use clap::{Parser, Subcommand};
use crate::cmd;
use redtrail::config::Config;
use redtrail::context::AppContext;
use redtrail::core;
use redtrail::error::Error;

#[derive(Parser)]
#[command(name = "rt", about = "Terminal activity capture and knowledge extraction")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command through a PTY, capturing output and extracting facts
    Proxy {
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Execute raw SQL against the database
    Sql {
        query: String,
        #[arg(long)]
        json: bool,
    },
    /// Run LLM-based extraction on a stored event
    Extract {
        event_id: i64,
        #[arg(long)]
        force: bool,
    },
}

pub fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    let db_path = core::db::global_db_path()?;
    let conn = core::db::open(db_path.to_str().unwrap())?;
    let cwd = std::env::current_dir()?;
    let workspace_path = cwd.to_string_lossy().to_string();
    let session_id = core::db::ensure_session(&conn, &workspace_path)?;
    let config = Config::default();
    let ctx = AppContext { conn, config, session_id };

    match cli.command {
        Commands::Proxy { command } => {
            cmd::proxy::run(&ctx, &cmd::proxy::ProxyArgs { command })
        }
        Commands::Sql { query, json } => {
            cmd::sql::run(&ctx, &cmd::sql::SqlArgs { query, json })
        }
        Commands::Extract { event_id, force } => {
            cmd::extract::run(&ctx, &cmd::extract::ExtractArgs { event_id, force })
        }
    }
}
