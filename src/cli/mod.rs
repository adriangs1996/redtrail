mod init;
pub mod kb;
pub mod status;
pub mod hypothesis;
pub mod evidence;
pub mod session;
pub mod scope;
pub mod config_cmd;
pub mod proxy;
pub mod env;
pub mod setup;
pub mod ingest;
pub mod report;
pub mod skill;

use clap::{Parser, Subcommand};
use crate::db::SessionOps;
use crate::error::Error;
use crate::workspace;

#[derive(Parser)]
#[command(name = "rt", about = "Redtrail — pentesting workspace manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "general")]
        goal: String,
        #[arg(long)]
        scope: Option<String>,
    },
    Kb {
        #[command(subcommand)]
        command: kb::KbCommands,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Hypothesis {
        #[command(subcommand)]
        command: hypothesis::HypothesisCommands,
    },
    Evidence {
        #[command(subcommand)]
        command: evidence::EvidenceCommands,
    },
    Session {
        #[command(subcommand)]
        command: session::SessionCommands,
    },
    Scope {
        #[command(subcommand)]
        command: scope::ScopeCommands,
    },
    Config {
        #[command(subcommand)]
        command: config_cmd::ConfigCommands,
    },
    Setup {
        #[command(subcommand)]
        command: Option<setup::SetupCommands>,
    },
    Ingest {
        file: String,
        #[arg(long)]
        tool: Option<String>,
    },
    Report {
        #[command(subcommand)]
        command: report::ReportCommands,
    },
    Pipeline,
    Env,
    Deactivate,
    Skill {
        #[command(subcommand)]
        command: skill::SkillCommands,
    },
}

fn resolve_session() -> Result<(impl crate::db::KnowledgeBase + crate::db::Hypotheses + crate::db::CommandLog + SessionOps, String), Error> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let db_path = workspace::db_path(&ws);
    let db = crate::db::open(db_path.to_str().unwrap())?;
    let session_id = db.active_session_id()?;
    Ok((db, session_id))
}

const KNOWN_SUBCOMMANDS: &[&str] = &[
    "init", "kb", "status", "hypothesis", "evidence",
    "session", "scope", "config", "setup", "ingest", "report", "pipeline", "env", "deactivate", "skill",
    "help", "--help", "-h", "--version", "-V",
];

pub fn run() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--" {
        return proxy::run(&args[2..]);
    }

    if args.len() > 1 && !KNOWN_SUBCOMMANDS.contains(&args[1].as_str()) {
        return proxy::run(&args[1..]);
    }

    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init { target, goal, scope }) => {
            init::run(target, goal, scope)
        }
        Some(Commands::Kb { command }) => {
            let (db, sid) = resolve_session()?;
            kb::run(&db, &sid, command)
        }
        Some(Commands::Status { json }) => {
            let (db, sid) = resolve_session()?;
            status::run(&db, &sid, json)
        }
        Some(Commands::Hypothesis { command }) => {
            let (db, sid) = resolve_session()?;
            hypothesis::run(&db, &sid, command)
        }
        Some(Commands::Evidence { command }) => {
            let (db, sid) = resolve_session()?;
            evidence::run(&db, &sid, command)
        }
        Some(Commands::Session { command }) => {
            let (db, sid) = resolve_session()?;
            session::run(&db, &sid, command)
        }
        Some(Commands::Scope { command }) => {
            let (db, sid) = resolve_session()?;
            scope::run(&db, &sid, command)
        }
        Some(Commands::Config { command }) => {
            config_cmd::run(command)
        }
        Some(Commands::Setup { command }) => {
            match command {
                None => setup::run_wizard(),
                Some(setup::SetupCommands::Status { json }) => setup::run_status(json),
                Some(setup::SetupCommands::Aliases(args)) => setup::run_aliases(args),
            }
        }
        Some(Commands::Ingest { file, tool }) => {
            let (db, sid) = resolve_session()?;
            ingest::run(&db, &sid, &file, tool)
        }
        Some(Commands::Report { command }) => {
            let (db, sid) = resolve_session()?;
            report::run(&db, &sid, command)
        }
        Some(Commands::Pipeline) => {
            println!("pipeline configurability deferred to v2");
            Ok(())
        }
        Some(Commands::Env) => {
            let (db, sid) = resolve_session()?;
            env::run(&db, &sid)
        }
        Some(Commands::Deactivate) => {
            env::deactivate()
        }
        Some(Commands::Skill { command }) => {
            skill::run(command)
        }
        None => {
            println!("rt: redtrail. Use --help for usage.");
            Ok(())
        }
    }
}
