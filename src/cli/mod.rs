pub(crate) mod ask;
pub mod config_cmd;
pub mod env;
pub mod evidence;
pub mod hypothesis;
pub mod ingest;
mod init;
pub mod kb;
pub mod proxy;
pub mod report;
pub mod scope;
pub mod session;
pub mod setup;
pub mod skill;
pub mod pipeline_cmd;
pub mod sql;
pub mod status;

use crate::db::SessionOps;
use crate::error::Error;
use crate::workspace;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rt",
    about = "Redtrail — pentesting workspace manager",
    long_about = "Redtrail — pentesting workspace manager\n\n\
        Orchestrates pentesting workflows with a knowledge base, hypothesis tracking,\n\
        evidence collection, and tool integration. Any unrecognized command is proxied\n\
        to the shell with automatic output capture and extraction.\n\n\
        Quick start:\n  rt init --target 10.10.10.1\n  eval \"$(rt env)\"\n  nmap -sV 10.10.10.1",
    after_help = "Any command not listed above is proxied to your shell and logged.\n\
        Use `rt -- <cmd>` to force proxy mode."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Initialize a new workspace in the current directory")]
    Init {
        #[arg(long, help = "Primary target IP or hostname")]
        target: Option<String>,
        #[arg(
            long,
            default_value = "general",
            help = "Assessment goal (e.g. general, ctf, webapp)"
        )]
        goal: String,
        #[arg(long, help = "CIDR scope restriction (e.g. 10.10.10.0/24)")]
        scope: Option<String>,
    },
    #[command(about = "Query and manage the knowledge base (hosts, ports, creds, flags, notes)")]
    Kb {
        #[command(subcommand)]
        command: kb::KbCommands,
    },
    #[command(
        about = "Show session metrics: hosts, ports, creds, flags, hypotheses",
        visible_alias = "st"
    )]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Track and manage attack hypotheses", visible_alias = "theory")]
    Hypothesis {
        #[command(subcommand)]
        command: hypothesis::HypothesisCommands,
    },
    #[command(
        about = "Record and manage evidence and findings",
        visible_alias = "ev"
    )]
    Evidence {
        #[command(subcommand)]
        command: evidence::EvidenceCommands,
    },
    #[command(about = "Manage workspace sessions", visible_alias = "sess")]
    Session {
        #[command(subcommand)]
        command: session::SessionCommands,
    },
    #[command(about = "Check whether an IP is within the defined scope")]
    Scope {
        #[command(subcommand)]
        command: scope::ScopeCommands,
    },
    #[command(
        about = "View and modify configuration (global and workspace)",
        visible_alias = "conf"
    )]
    Config {
        #[command(subcommand)]
        command: config_cmd::ConfigCommands,
    },
    #[command(about = "Run the interactive setup wizard or manage tool aliases")]
    Setup {
        #[command(subcommand)]
        command: Option<setup::SetupCommands>,
    },
    #[command(
        about = "Import tool output files into the knowledge base",
        visible_alias = "eat"
    )]
    Ingest {
        #[arg(help = "Path to tool output file (nmap, gobuster, nuclei, nikto, feroxbuster)")]
        file: String,
        #[arg(long, help = "Override auto-detected tool name")]
        tool: Option<String>,
    },
    #[command(
        about = "Generate a penetration test report from session data",
        visible_alias = "rep"
    )]
    Report {
        #[command(subcommand)]
        command: report::ReportCommands,
    },
    #[command(about = "Pipeline management")]
    Pipeline {
        #[command(subcommand)]
        command: pipeline_cmd::PipelineCommands,
    },
    #[command(about = "Print shell commands to activate the redtrail environment")]
    Env,
    #[command(
        about = "Print shell commands to deactivate the redtrail environment",
        visible_alias = "deact"
    )]
    Deactivate,
    #[command(about = "Manage redtrail skills (create, test, install, remove)")]
    Skill {
        #[command(subcommand)]
        command: skill::SkillCommands,
    },
    #[command(about = "Ask the LLM with full session context and conversation history")]
    Ask {
        #[arg(help = "Your question or instruction")]
        message: Option<String>,
        #[arg(long, help = "Clear conversation history and exit")]
        clear: bool,
        #[arg(long, help = "Override LLM model for this request")]
        model: Option<String>,
        #[arg(long, help = "Override auto-detected skill (e.g. redtrail-recon)")]
        skill: Option<String>,
        #[arg(long, help = "Suppress skill auto-detection")]
        no_skill: bool,
    },
    #[command(
        about = "One-shot LLM query with session context (no history)",
        visible_alias = "q"
    )]
    Query {
        #[arg(help = "Your question")]
        message: String,
        #[arg(long, help = "Override LLM model for this request")]
        model: Option<String>,
        #[arg(long, help = "Override auto-detected skill (e.g. redtrail-recon)")]
        skill: Option<String>,
        #[arg(long, help = "Suppress skill auto-detection")]
        no_skill: bool,
    },
    #[command(about = "Run SQL against the redtrail database")]
    Sql {
        #[arg(help = "SQL statement to execute")]
        sql: Option<String>,
        #[arg(long, help = "Read SQL from file instead")]
        file: Option<String>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

fn resolve_session() -> Result<
    (
        impl crate::db::KnowledgeBase + crate::db::Hypotheses + crate::db::CommandLog + SessionOps,
        String,
    ),
    Error,
> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let db_path = workspace::db_path(&ws);
    let db = crate::db::open(db_path.to_str().unwrap())?;
    let session_id = db.active_session_id()?;
    Ok((db, session_id))
}

const KNOWN_SUBCOMMANDS: &[&str] = &[
    "init",
    "kb",
    "status",
    "hypothesis",
    "evidence",
    "session",
    "scope",
    "config",
    "setup",
    "ingest",
    "report",
    "pipeline",
    "extract",
    "env",
    "deactivate",
    "skill",
    "ask",
    "query",
    "sql",
    "st",
    "theory",
    "ev",
    "sess",
    "conf",
    "eat",
    "rep",
    "deact",
    "q",
    "help",
    "--help",
    "-h",
    "--version",
    "-V",
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
        Some(Commands::Init {
            target,
            goal,
            scope,
        }) => init::run(target, goal, scope),
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
        Some(Commands::Config { command }) => config_cmd::run(command),
        Some(Commands::Setup { command }) => match command {
            None => setup::run_wizard(),
            Some(setup::SetupCommands::Status { json }) => setup::run_status(json),
            Some(setup::SetupCommands::Aliases(args)) => setup::run_aliases(args),
        },
        Some(Commands::Ingest { file, tool }) => {
            let (db, sid) = resolve_session()?;
            ingest::run(&db, &sid, &file, tool)
        }
        Some(Commands::Report { command }) => {
            let (db, sid) = resolve_session()?;
            report::run(&db, &sid, command)
        }
        Some(Commands::Pipeline { command }) => pipeline_cmd::run(command),
        Some(Commands::Env) => {
            let (db, sid) = resolve_session()?;
            env::run(&db, &sid)
        }
        Some(Commands::Deactivate) => env::deactivate(),
        Some(Commands::Skill { command }) => skill::run(command),
        Some(Commands::Ask {
            message,
            clear,
            model,
            skill,
            no_skill,
        }) => ask::run(
            message.as_deref(),
            true,
            clear,
            model.as_deref(),
            skill.as_deref(),
            no_skill,
        ),
        Some(Commands::Query {
            message,
            model,
            skill,
            no_skill,
        }) => ask::run(
            Some(&message),
            false,
            false,
            model.as_deref(),
            skill.as_deref(),
            no_skill,
        ),
        Some(Commands::Sql { sql, file, json }) => match (sql, file) {
            (_, Some(path)) => sql::run_file(&path, json),
            (Some(query), _) => sql::run(&query, json),
            (None, None) => Err(Error::Config("provide SQL or --file".into())),
        },
        None => {
            println!("rt: redtrail. Use --help for usage.");
            Ok(())
        }
    }
}
