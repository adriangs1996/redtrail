use std::process;

use clap::{Parser, Subcommand};
use colored::Colorize;

use redtrail::{
    AnthropicApiConfig, AssessmentDepth, Backend, Criterion, CriterionCheck, ExecMode, GoalType,
    LlmConfig, LlmProvider, OllamaConfig, ScanSession, SessionGoal, Target, Db, Error,
    create_provider, query_agent, tui::App,
};

#[derive(Parser)]
#[command(
    name = "Redtrail",
    version = "v0.1.0",
    about = "Gray Box Agentic Security Scanner"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show scan status and knowledge base summary
    Status {
        #[arg(long)]
        scan: Option<String>,

        #[arg(long)]
        list: bool,
    },

    /// Query scan results interactively or with a question
    Query {
        #[arg(long)]
        scan: Option<String>,

        #[arg(long)]
        question: Option<String>,

        #[arg(long, default_value = "anthropic", value_parser = ["anthropic", "ollama"])]
        llm: String,

        #[arg(long)]
        ollama_model: Option<String>,
    },

    /// Interactive driver mode — you direct, AI advises
    Drive {
        /// Target URL (optional unless --hosts is provided)
        #[arg(long)]
        target: Option<String>,

        /// Comma-separated network hosts: IPs or CIDR ranges
        #[arg(long, value_delimiter = ',')]
        hosts: Vec<String>,

        /// Show full agent reasoning output
        #[arg(long)]
        verbose: bool,

        /// LLM provider: "anthropic" (default) or "ollama" (local)
        #[arg(long, default_value = "anthropic", value_parser = ["ollama", "anthropic"])]
        llm: String,

        /// Ollama model override (default: deepseek-r1:8b)
        #[arg(long)]
        ollama_model: Option<String>,

        /// Session goal type
        #[arg(long, value_parser = ["capture-flags", "gain-access", "vuln-assessment", "custom"])]
        goal: Option<String>,

        /// Flag pattern regex (used with --goal capture-flags)
        #[arg(long)]
        flag_pattern: Option<String>,

        /// Expected number of flags (used with --goal capture-flags)
        #[arg(long)]
        expected_flags: Option<u32>,

        /// Target privilege level (used with --goal gain-access)
        #[arg(long)]
        privilege: Option<String>,

        /// Custom objective description (used with --goal custom)
        #[arg(long)]
        objective: Option<String>,

        /// Assessment depth (used with --goal vuln-assessment)
        #[arg(long, value_parser = ["quick", "standard", "deep"])]
        depth: Option<String>,
    },

    Shell {
        #[arg(long, default_value = "default")]
        session: String,

        #[arg(long, value_delimiter = ',')]
        hosts: Vec<String>,

        #[arg(long)]
        target: Option<String>,

        #[arg(long, default_value = "anthropic", value_parser = ["anthropic", "ollama"])]
        llm: String,

        #[arg(long)]
        model: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status { scan, list } => {
            run_status(scan, list);
        }
        Commands::Query {
            scan,
            question,
            llm,
            ollama_model,
        } => {
            run_query(scan, question, llm, ollama_model).await;
        }
        Commands::Drive {
            target,
            hosts,
            verbose,
            llm,
            ollama_model,
            goal,
            flag_pattern,
            expected_flags,
            privilege,
            objective,
            depth,
        } => {
            let session_goal = parse_session_goal(
                goal,
                flag_pattern,
                expected_flags,
                privilege,
                objective,
                depth,
                &target,
                &hosts,
            );
            run_drive(target, hosts, verbose, session_goal, llm, ollama_model).await;
        }
        Commands::Shell { session, hosts, target, llm, model } => {
            run_shell(session, hosts, target, llm, model).await;
        }
    }
}

/// Create the LLM provider from CLI flags.
fn create_llm_provider(
    llm: &str,
    ollama_model: Option<String>,
) -> Result<Box<dyn LlmProvider>, Error> {
    let config = match llm {
        "ollama" => {
            let mut config = OllamaConfig::default();
            if let Some(model) = ollama_model {
                config.model = model;
            }
            LlmConfig::Ollama(config)
        }
        _ => {
            let api_config = AnthropicApiConfig::from_env().unwrap_or_else(|e| {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            });
            LlmConfig::AnthropicApi(api_config)
        }
    };
    Ok(create_provider(config)?)
}

fn load_session(scan_id: Option<String>) -> ScanSession {
    let db = match Db::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("{} Failed to open database: {}", "[ERROR]".red().bold(), e);
            process::exit(1);
        }
    };

    match scan_id {
        Some(id) => match db.load_session(&id) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            }
        },
        None => match db.latest_session() {
            Ok(Some(s)) => s,
            Ok(None) => {
                eprintln!(
                    "{} No scan sessions found. Run 'redtrail drive' first.",
                    "[ERROR]".red().bold()
                );
                process::exit(1);
            }
            Err(e) => {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            }
        },
    }
}

fn run_status(scan_id: Option<String>, list: bool) {
    if list {
        let db = match Db::open() {
            Ok(db) => db,
            Err(e) => {
                eprintln!("{} Failed to open database: {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            }
        };

        let sessions = match db.list_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            }
        };

        if sessions.is_empty() {
            println!("No scan sessions found.");
            return;
        }

        println!(
            "{:<10} {:<22} {:<30} {:>6} {:>8} {:<12}",
            "ID", "Date", "Target", "Turns", "Findings", "Status"
        );
        println!("{}", "─".repeat(92));
        for s in &sessions {
            println!(
                "{:<10} {:<22} {:<30} {:>6} {:>8} {:<12}",
                &s.id[..8.min(s.id.len())],
                &s.created_at[..22.min(s.created_at.len())],
                s.target_url.as_deref().unwrap_or("N/A"),
                s.total_turns_used,
                s.findings_count,
                format!("{:?}", s.status),
            );
        }
        return;
    }

    let session = load_session(scan_id);

    println!("{}", "Redtrail Scan Status".bold());
    println!("{}", "═".repeat(50).dimmed());
    println!(
        "  {}: {}",
        "Session".dimmed(),
        &session.id[..8.min(session.id.len())]
    );
    println!(
        "  {}: {}",
        "Target".dimmed(),
        session.target_url.as_deref().unwrap_or("N/A")
    );
    if !session.target_hosts.is_empty() {
        println!(
            "  {}: {}",
            "Hosts".dimmed(),
            session.target_hosts.join(", ")
        );
    }
    println!("  {}: {:?}", "Status".dimmed(), session.status);
    println!(
        "  {}: {} / {}",
        "Turns".dimmed(),
        session.total_turns_used,
        session.max_turns_configured
    );
    println!("  {}: {}", "LLM".dimmed(), session.llm_provider);
    println!("  {}: {}", "Date".dimmed(), session.created_at);
    println!("  {}: {}", "Findings".dimmed(), session.findings.len());

    let kb_summary = session.knowledge.to_context_summary();
    if !kb_summary.is_empty() {
        println!("{kb_summary}");
    }
}

async fn run_query(
    scan_id: Option<String>,
    question: Option<String>,
    llm: String,
    ollama_model: Option<String>,
) {
    let session = load_session(scan_id);

    let provider = match create_llm_provider(&llm, ollama_model) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} {}", "[ERROR]".red().bold(), e);
            process::exit(1);
        }
    };
    let provider: std::sync::Arc<dyn LlmProvider> = std::sync::Arc::from(provider);

    match question {
        Some(q) => match query_agent::query_oneshot(&session, &provider, &q).await {
            Ok(answer) => println!("{answer}"),
            Err(e) => {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            }
        },
        None => {
            if let Err(e) = query_agent::query_repl(&session, &provider).await {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                process::exit(1);
            }
        }
    }
}

/// Parse CLI goal flags into a `SessionGoal`.
#[allow(clippy::too_many_arguments)]
fn parse_session_goal(
    goal: Option<String>,
    flag_pattern: Option<String>,
    expected_flags: Option<u32>,
    privilege: Option<String>,
    objective: Option<String>,
    depth: Option<String>,
    target_url: &Option<String>,
    hosts: &[String],
) -> Option<SessionGoal> {
    let goal_str = goal?;
    let (goal_type, description, criteria) = match goal_str.as_str() {
        "capture-flags" => {
            let pattern = flag_pattern.unwrap_or_else(|| r"FLAG\{[^}]+\}".to_string());
            let count = expected_flags;
            let desc = match count {
                Some(n) => format!("Capture {n} flags matching pattern: {pattern}"),
                None => format!("Capture flags matching pattern: {pattern}"),
            };
            let criteria = vec![Criterion {
                description: desc.clone(),
                check: CriterionCheck::FlagsCaptured {
                    min_count: count.unwrap_or(1),
                },
                met: false,
            }];
            (
                GoalType::CaptureFlags {
                    flag_pattern: pattern,
                    expected_count: count,
                },
                desc,
                criteria,
            )
        }
        "gain-access" => {
            let priv_level = privilege.unwrap_or_else(|| "root".to_string());
            let host = target_url
                .clone()
                .or_else(|| hosts.first().cloned())
                .unwrap_or_else(|| "unknown".to_string());
            let desc = format!("Gain {priv_level} access on {host}");
            let criteria = vec![Criterion {
                description: desc.clone(),
                check: CriterionCheck::AccessObtained {
                    host: host.clone(),
                    min_privilege: priv_level.clone(),
                },
                met: false,
            }];
            (
                GoalType::GainAccess {
                    target_host: host,
                    privilege_level: priv_level,
                },
                desc,
                criteria,
            )
        }
        "vuln-assessment" => {
            let assessment_depth = match depth.as_deref() {
                Some("quick") => AssessmentDepth::Quick,
                Some("deep") => AssessmentDepth::Deep,
                _ => AssessmentDepth::Standard,
            };
            let depth_label = match &assessment_depth {
                AssessmentDepth::Quick => "quick",
                AssessmentDepth::Standard => "standard",
                AssessmentDepth::Deep => "deep",
            };
            let scope: Vec<String> = hosts.to_vec();
            let desc = format!("Vulnerability assessment ({depth_label} depth)");
            let criteria = vec![Criterion {
                description: "Find vulnerabilities".to_string(),
                check: CriterionCheck::VulnsFound {
                    min_count: 1,
                    min_severity: "low".to_string(),
                },
                met: false,
            }];
            (
                GoalType::VulnerabilityAssessment {
                    scope,
                    depth: assessment_depth,
                },
                desc,
                criteria,
            )
        }
        "custom" => {
            let obj = objective.unwrap_or_else(|| "Custom objective".to_string());
            let desc = obj.clone();
            let criteria = vec![Criterion {
                description: obj.clone(),
                check: CriterionCheck::Custom {
                    description: obj.clone(),
                },
                met: false,
            }];
            (GoalType::Custom { objective: obj }, desc, criteria)
        }
        _ => unreachable!("clap validates goal values"),
    };

    Some(SessionGoal {
        goal_type,
        description,
        success_criteria: criteria,
        ..SessionGoal::default()
    })
}

async fn run_shell(
    session_name: String,
    hosts: Vec<String>,
    target_url: Option<String>,
    llm: String,
    model: Option<String>,
) {
    use redtrail::db_v2::DbV2;
    use redtrail::workflows::session::{SessionContext, SessionWorkflow};

    let rt_dir = dirs::home_dir()
        .map(|h| h.join(".redtrail"))
        .unwrap_or_else(|| std::path::PathBuf::from(".redtrail"));
    let log_dir = rt_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "redtrail.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with_ansi(false)
        .with_target(true)
        .init();

    tracing::info!("redtrail shell starting — session={}", session_name);

    let db_path = rt_dir.join("redtrail_v2.db");

    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let db = std::sync::Arc::new(std::sync::Mutex::new(
        DbV2::open(db_path.to_str().unwrap()).unwrap_or_else(|e| {
            eprintln!("{} {}", "[ERROR]".red().bold(), e);
            process::exit(1);
        })
    ));

    let existing = {
        let db_guard = db.lock().unwrap();
        SessionWorkflow::load_by_name(&db_guard, &session_name).ok()
    };
    let session = match existing {
        Some(mut s) => {
            if !hosts.is_empty() { s.target.hosts = hosts; }
            if let Some(url) = target_url { s.target.base_url = Some(url); }
            if let Some(m) = model { s.llm_model = m; }
            s.llm_provider = match llm.as_str() {
                "ollama" => "ollama".into(),
                _ => "anthropic-api".into(),
            };
            s
        }
        None => {
            let mut s = SessionContext::new(session_name);
            s.target.hosts = hosts;
            s.target.base_url = target_url;
            if let Some(m) = model { s.llm_model = m; }
            s.llm_provider = match llm.as_str() {
                "ollama" => "ollama".into(),
                _ => "anthropic-api".into(),
            };
            s
        }
    };
    {
        let db_guard = db.lock().unwrap();
        SessionWorkflow::save(&db_guard, &session).unwrap();
    }

    let llm_provider: Option<std::sync::Arc<dyn redtrail::LlmProvider>> = {
        let config = match session.llm_provider.as_str() {
            "anthropic-api" => {
                redtrail::AnthropicApiConfig::from_env().ok().map(|mut cfg| {
                    cfg.model = session.llm_model.clone();
                    redtrail::LlmConfig::AnthropicApi(cfg)
                })
            }
            "ollama" => Some(redtrail::LlmConfig::Ollama(redtrail::OllamaConfig {
                model: session.llm_model.clone(),
                ..Default::default()
            })),
            _ => None,
        };
        config.and_then(|c| redtrail::create_provider(c).ok().map(std::sync::Arc::from))
    };

    let llm_tools: Option<std::sync::Arc<redtrail::agent::tools::ToolRegistry>> = llm_provider.as_ref().map(|_| {
        let kb = std::sync::Arc::new(tokio::sync::RwLock::new(
            redtrail::agent::knowledge::KnowledgeBase::default(),
        ));
        let mut registry = redtrail::agent::tools::ToolRegistry::new();
        registry.register(redtrail::agent::tools::run_command::run_command(kb.clone()));
        registry.register(redtrail::agent::tools::query_kb::query_kb(kb));
        registry.register(redtrail::agent::tools::get_command_result::get_command_result(db.clone()));
        std::sync::Arc::new(registry)
    });

    let mut app = redtrail::tui::App::new(session, db, llm_provider, llm_tools);
    if let Err(e) = app.run().await {
        eprintln!("{} {}", "[ERROR]".red().bold(), e);
        process::exit(1);
    }
}

async fn run_drive(
    target_url: Option<String>,
    hosts: Vec<String>,
    _verbose: bool,
    _session_goal: Option<SessionGoal>,
    llm: String,
    ollama_model: Option<String>,
) {
    let target = Target {
        base_url: target_url,
        hosts,
        exec_mode: ExecMode::Local,
        auth_token: None,
        scope: vec![],
    };

    // Create the LLM provider
    let provider: std::sync::Arc<dyn LlmProvider> = {
        let p = create_llm_provider(&llm, ollama_model).unwrap_or_else(|e| {
            eprintln!("{} {}", "[ERROR]".red().bold(), e);
            process::exit(1);
        });
        std::sync::Arc::from(p)
    };

    let (event_sender, _event_receiver) = tokio::sync::mpsc::channel(64);
    let (_command_sender, command_receiver) = tokio::sync::mpsc::channel(64);

    let db = Db::open().ok();
    let knowledge = std::sync::Arc::new(tokio::sync::RwLock::new(
        redtrail::agent::knowledge::KnowledgeBase::new(),
    ));
    let mut registry = redtrail::agent::tools::ToolRegistry::new();
    registry.register(redtrail::agent::tools::run_command::run_command(
        knowledge.clone(),
    ));
    registry.register(redtrail::agent::tools::query_kb::query_kb(knowledge.clone()));
    let tools = std::sync::Arc::new(registry);
    let mut backend = Backend::new(
        target,
        event_sender,
        command_receiver,
        db,
        provider,
        tools,
        knowledge,
    );

    let session = redtrail::workflows::session::SessionContext::new("default".to_string());
    let rt_db = redtrail::db_v2::DbV2::open_in_memory().unwrap_or_else(|e| {
        eprintln!("{} {}", "[ERROR]".red().bold(), e);
        process::exit(1);
    });
    let rt_db = std::sync::Arc::new(std::sync::Mutex::new(rt_db));
    let mut app = App::new(session, rt_db, None, None);

    let backend_handle = tokio::spawn(async move { backend.run().await });

    if let Err(e) = app.run().await {
        eprintln!("{} {}", "[ERROR]".red().bold(), e);
        process::exit(1);
    }

    backend_handle.abort();
}
