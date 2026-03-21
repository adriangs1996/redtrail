use crate::agent::extraction::{ExtractionInput, build_extraction_agent};
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
    let model = crate::agent::create_model(&config)?;
    let prompt = input.to_prompt();
    let conn = Arc::new(Mutex::new(conn));
    let agent = build_extraction_agent(model, conn.clone(), session_id, cwd);

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::Config(format!("tokio runtime: {e}")))?;

    match rt.block_on(agent.run(&prompt)) {
        Ok(_) => {
            let c = conn.lock().unwrap();
            commands::update_extraction_status(&c, cmd_id, "done")?;
        }
        Err(e) => {
            let c = conn.lock().unwrap();
            commands::update_extraction_status(&c, cmd_id, "failed")?;
            return Err(Error::Config(format!("extraction agent: {e}")));
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
