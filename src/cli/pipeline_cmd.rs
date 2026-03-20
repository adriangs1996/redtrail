use crate::config::Config;
use crate::db::CommandLog;

use crate::error::Error;
use crate::workspace;
use clap::Subcommand;

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
    let db = crate::db::open(db_path.to_str().unwrap())?;

    let session_id = db.get_command_for_extraction(cmd_id)?.0;

    let config = Config::resolved(&ws)?;
    crate::extraction::extract_sync(&db, &session_id, cmd_id, &config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::{CommandLog, SessionOps, open_in_memory};

    #[test]
    fn test_extract_skips_empty_output() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        let cmd_id = db.insert_command("s1", "echo hi", Some("echo")).unwrap();
        db.finish_command(cmd_id, 0, 100, "").unwrap();

        let config = crate::config::Config::default();
        let result = crate::extraction::extract_sync(&db, "s1", cmd_id, &config);
        assert!(result.is_ok());
    }
}
