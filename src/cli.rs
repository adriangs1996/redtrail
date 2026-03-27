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
        /// Show only commands from today
        #[arg(long)]
        today: bool,
        /// Full-text search across commands and output
        #[arg(long)]
        search: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Execute a raw SQL query against the database
    Query {
        /// The SQL query (SELECT only)
        sql: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List recent terminal sessions
    Sessions,
    /// Show commands from a specific session
    Session {
        /// Session ID
        id: String,
    },
    /// Show database status and statistics
    Status,
    /// Delete captured data
    Forget {
        /// Delete a specific command by ID
        #[arg(long)]
        command: Option<String>,
        /// Delete all commands in a session
        #[arg(long)]
        session: Option<String>,
        /// Delete commands since a timestamp
        #[arg(long)]
        since: Option<i64>,
    },
    /// Generate a new session ID (called by shell hooks)
    #[command(hide = true)]
    SessionId,
    /// Record a command execution (called by shell hooks)
    #[command(hide = true)]
    Capture {
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        command: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        exit_code: Option<i32>,
        #[arg(long)]
        ts_start: i64,
        #[arg(long)]
        ts_end: Option<i64>,
        #[arg(long)]
        shell: Option<String>,
        #[arg(long)]
        hostname: Option<String>,
    },
    /// Export captured data as JSON
    Export {
        /// Export commands since this timestamp
        #[arg(long)]
        since: Option<i64>,
    },
    /// View or modify configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a configuration value
    Set {
        key: String,
        value: String,
    },
}

fn config_path() -> String {
    std::env::var("REDTRAIL_CONFIG").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.config/redtrail/config.yaml")
    })
}

fn db_path() -> Option<String> {
    std::env::var("REDTRAIL_DB").ok().or_else(|| {
        db::global_db_path().ok().map(|p| p.to_string_lossy().to_string())
    })
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
        Commands::History { failed, cmd, cwd, today, search, json } => {
            let conn = open_db()?;
            cmd::history::run(&conn, &cmd::history::HistoryArgs {
                failed,
                cmd: cmd.as_deref(),
                cwd: cwd.as_deref(),
                today,
                search: search.as_deref(),
                json,
            })
        }
        Commands::Query { sql, json } => {
            let conn = open_db()?;
            cmd::query::run(&conn, &sql, json)
        }
        Commands::Sessions => {
            let conn = open_db()?;
            cmd::sessions::list(&conn)
        }
        Commands::Session { id } => {
            let conn = open_db()?;
            cmd::sessions::detail(&conn, &id)
        }
        Commands::Status => {
            let conn = open_db()?;
            cmd::status::run(&conn, db_path().as_deref())
        }
        Commands::Forget { command, session, since } => {
            let conn = open_db()?;
            cmd::forget::run(&conn, &cmd::forget::ForgetArgs {
                command: command.as_deref(),
                session: session.as_deref(),
                since,
            })
        }
        Commands::SessionId => {
            let conn = open_db()?;
            let cwd = std::env::current_dir().ok()
                .and_then(|p| p.to_str().map(String::from));
            let hostname = std::env::var("HOSTNAME").or_else(|_| std::env::var("HOST")).ok();
            let shell = std::env::var("SHELL").ok();
            let id = db::create_session(&conn, &db::NewSession {
                cwd_initial: cwd.as_deref(),
                hostname: hostname.as_deref(),
                shell: shell.as_deref(),
                source: "human",
            })?;
            print!("{id}");
            Ok(())
        }
        Commands::Capture { session_id, command, cwd, exit_code, ts_start, ts_end, shell, hostname } => {
            let conn = open_db()?;
            cmd::capture::run(&conn, &cmd::capture::CaptureArgs {
                session_id: &session_id,
                command: &command,
                cwd: cwd.as_deref(),
                exit_code,
                ts_start,
                ts_end,
                shell: shell.as_deref(),
                hostname: hostname.as_deref(),
            })
        }
        Commands::Export { since } => {
            let conn = open_db()?;
            cmd::export::run(&conn, since)
        }
        Commands::Config { action } => {
            let config_path = config_path();
            match action {
                None => cmd::config::view(&config_path),
                Some(ConfigAction::Set { key, value }) => cmd::config::set(&config_path, &key, &value),
            }
        }
    }
}
