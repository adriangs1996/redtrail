use crate::agent::extraction::{ExtractionInput, build_extraction_agent};
use crate::agent::strategist::{
    StrategistInput, build_strategist_agent, collect_new_records, collect_suggestions,
};
use crate::config::Config;
use crate::db::commands;
use crate::error::Error;
use crate::workspace;
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
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let db_path = workspace::db_path(&ws);
    let conn = Connection::open(db_path).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
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

    let config = Config::resolved(&ws)?;
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

    match rt.block_on(strat_agent.run(&strat_prompt)) {
        Ok(strat_response) => {
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
        Err(_) => {}
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
