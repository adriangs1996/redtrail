use crate::agent::extraction::{ExtractionInput, build_extraction_agent};
use crate::agent::providers::ClaudeCodeProvider;
use crate::agent::strategist::{
    StrategistInput, build_strategist_agent, collect_new_records, collect_suggestions,
};
use crate::config::Config;
use crate::db::commands;
use crate::error::Error;
use crate::resolve;
use clap::Subcommand;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[derive(Subcommand)]
pub enum PipelineCommands {
    #[command(about = "Run LLM extraction on a recorded command")]
    Extract {
        #[arg(help = "Command history ID to extract")]
        cmd_id: i64,
    },
}

pub fn run(command: PipelineCommands) -> Result<(), Error> {
    match command {
        PipelineCommands::Extract { cmd_id } => run_extract(cmd_id),
    }
}

fn run_extract(cmd_id: i64) -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let db_path = resolve::global_db_path()?;
    let conn = Connection::open(&db_path).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(crate::db::SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;

    let (session_id, command, tool, output) = commands::get_for_extraction(&conn, cmd_id)?;

    let input = ExtractionInput {
        command,
        tool,
        output: output.unwrap_or_default(),
    };

    if input.should_skip() {
        commands::update_extraction_status(&conn, cmd_id, "skipped")?;
        return Ok(());
    }

    let config = Config::resolved(&conn, &session_id)?;

    if config.general.llm_provider == "claude-code" {
        let conn = Arc::new(Mutex::new(conn));
        return run_extract_claude_code(cmd_id, &session_id, &input, conn, cwd);
    }

    let model = match crate::agent::create_model(&config) {
        Ok(m) => m,
        Err(e) => {
            commands::update_extraction_status(&conn, cmd_id, "failed")?;
            return Err(e);
        }
    };
    let prompt = input.to_prompt();
    let conn = Arc::new(Mutex::new(conn));
    let agent = build_extraction_agent(model, conn.clone(), session_id.clone(), cwd.clone());

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::Config(format!("tokio runtime: {e}")))?;

    let extraction_response = match rt.block_on(agent.run(&prompt)) {
        Ok(resp) => {
            let c = conn.lock().unwrap();
            commands::update_extraction_status(&c, cmd_id, "done")?;
            resp
        }
        Err(e) => {
            let c = conn.lock().unwrap();
            commands::update_extraction_status(&c, cmd_id, "failed")?;
            return Err(Error::Config(format!("extraction agent: {e}")));
        }
    };

    let calls = extraction_response.options.tool_calls().unwrap_or_default();
    let results = extraction_response.options.tool_results().unwrap_or_default();
    let new_records = collect_new_records(&calls, &results);

    if new_records.is_empty() {
        return Ok(());
    }

    let strat_model = crate::agent::create_model(&config)?;
    let strat_input = StrategistInput { new_records };
    let strat_prompt = strat_input.to_prompt();
    let strat_agent = build_strategist_agent(strat_model, conn.clone(), session_id, cwd)?;

    if let Ok(strat_response) = rt.block_on(strat_agent.run(&strat_prompt)) {
        let strat_results = strat_response.options.tool_results().unwrap_or_default();
        let suggestions = collect_suggestions(&strat_results);
        for s in &suggestions {
            let text = s["text"].as_str().unwrap_or("");
            let priority = s["priority"].as_str().unwrap_or("medium");
            let indicator = match priority {
                "critical" => "\x1b[31m[!!!]\x1b[0m",
                "high" => "\x1b[33m[!!]\x1b[0m",
                "medium" => "\x1b[36m[!]\x1b[0m",
                _ => "\x1b[2m[·]\x1b[0m",
            };
            eprintln!("[rt] {indicator} {text}");
        }
    }

    Ok(())
}

fn run_extract_claude_code(
    cmd_id: i64,
    session_id: &str,
    input: &ExtractionInput,
    conn: Arc<Mutex<Connection>>,
    cwd: std::path::PathBuf,
) -> Result<(), Error> {
    let rt_bin = std::env::current_exe()
        .map_err(|e| Error::Config(format!("current_exe: {e}")))?;
    let rt_path = rt_bin.display();

    let system = format!(
        "You are an extraction agent for a penetration testing knowledge base.\n\
        Parse the tool output and insert ALL findings into the database using SQL.\n\
        Use the Bash tool to run the commands below.\n\n\
        Session ID: {session_id}\n\n\
        To insert a host:\n\
        {rt_path} sql \"INSERT OR IGNORE INTO hosts (session_id, ip, os, status) \
        VALUES ('{session_id}', '<IP>', '<OS>', 'up')\"\n\n\
        To insert a port (host must exist first):\n\
        {rt_path} sql \"INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service, version) \
        VALUES ('{session_id}', (SELECT id FROM hosts WHERE session_id='{session_id}' AND ip='<IP>'), \
        <PORT>, '<PROTOCOL>', '<SERVICE>', '<VERSION>')\"\n\n\
        Rules:\n\
        - Insert ALL hosts and ALL ports found in the output\n\
        - Always insert hosts before their ports\n\
        - Use exact values from the output (IPs, port numbers, service names, versions)\n\
        - For protocol, use 'tcp' or 'udp'\n\
        - Do NOT hallucinate data not present in the output"
    );

    let prompt = format!(
        "Extract all hosts and ports from this output:\n\nCommand: {}\nTool: {}\n\nOutput:\n{}",
        input.command,
        input.tool.as_deref().unwrap_or("unknown"),
        input.output,
    );

    let provider = ClaudeCodeProvider::new()
        .with_cwd(cwd)
        .with_max_turns(3);

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::Config(format!("tokio runtime: {e}")))?;

    match rt.block_on(provider.run_claude(&prompt, Some(&system))) {
        Ok(_) => {
            let c = conn.lock().unwrap();
            commands::update_extraction_status(&c, cmd_id, "done")?;
        }
        Err(e) => {
            let c = conn.lock().unwrap();
            commands::update_extraction_status(&c, cmd_id, "failed")?;
            return Err(Error::Config(format!("claude-code extraction: {e}")));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::agent::extraction::ExtractionInput;

    #[test]
    fn test_extract_skips_empty_output() {
        let input = ExtractionInput {
            command: "echo hi".into(),
            tool: Some("echo".into()),
            output: String::new(),
        };
        assert!(input.should_skip());
    }

    #[test]
    fn test_extract_skips_whitespace_output() {
        let input = ExtractionInput {
            command: "echo".into(),
            tool: None,
            output: "   \n  ".into(),
        };
        assert!(input.should_skip());
    }

    #[test]
    fn test_extract_does_not_skip_real_output() {
        let input = ExtractionInput {
            command: "nmap -sV 10.10.10.1".into(),
            tool: Some("nmap".into()),
            output: "22/tcp open ssh OpenSSH 8.9".into(),
        };
        assert!(!input.should_skip());
    }
}
