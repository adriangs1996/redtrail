use clap::{Parser, Subcommand};
use redtrail::cmd;
use redtrail::core::db;
use redtrail::error::Error;

#[derive(Parser)]
#[command(name = "redtrail", about = "Terminal intelligence engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Output shell hook script for the given shell
    Init {
        shell: String,
    },
    /// Show command history
    History {
        /// Show only failed commands (non-zero exit code)
        #[arg(long)]
        failed: bool,
        /// Filter by command binary (e.g., git, docker)
        #[arg(long)]
        cmd: Option<String>,
        /// Filter by working directory
        #[arg(long)]
        cwd: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn open_db() -> Result<rusqlite::Connection, Error> {
    if let Ok(path) = std::env::var("REDTRAIL_DB") {
        db::open(&path)
    } else {
        let path = db::global_db_path()?;
        db::open(path.to_str().unwrap())
    }
}

pub fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { shell } => cmd::init::run(&shell),
        Commands::History { failed, cmd, cwd, json } => {
            let conn = open_db()?;
            cmd::history::run(&conn, &cmd::history::HistoryArgs {
                failed,
                cmd: cmd.as_deref(),
                cwd: cwd.as_deref(),
                json,
            })
        }
    }
}
