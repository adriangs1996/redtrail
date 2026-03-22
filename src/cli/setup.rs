use crate::config::Config;
use crate::db::config::set_global_config;
use crate::error::Error;
use crate::resolve;
use clap::{Args, Subcommand};

const KNOWN_TOOLS: &[&str] = &[
    "nmap",
    "gobuster",
    "feroxbuster",
    "ffuf",
    "dirb",
    "nikto",
    "sqlmap",
    "hydra",
    "crackmapexec",
    "whatweb",
    "nuclei",
    "john",
    "hashcat",
    "curl",
    "wget",
    "ssh",
    "scp",
    "nc",
    "netcat",
    "enum4linux",
    "responder",
    "wfuzz",
];

#[derive(Subcommand)]
pub enum SetupCommands {
    #[command(about = "Show current setup status (installed tools, config path, autonomy mode)")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Manage tool aliases that get proxied through rt")]
    Aliases(AliasesArgs),
}

#[derive(Args)]
pub struct AliasesArgs {
    #[arg(long, help = "Add a tool alias (e.g. nmap, gobuster)")]
    pub add: Option<String>,
    #[arg(long, help = "Remove a tool alias")]
    pub remove: Option<String>,
    #[arg(long, help = "List all configured aliases")]
    pub list: bool,
}

fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.split('/').next_back().map(String::from))
        .unwrap_or_else(|| "unknown".to_string())
}

fn scan_tools() -> Vec<String> {
    KNOWN_TOOLS
        .iter()
        .filter(|t| {
            std::process::Command::new("which")
                .arg(t)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .collect()
}

fn save_config_to_db(config: &Config) -> Result<(), Error> {
    let ctx = resolve::resolve_global()?;
    let conn = &ctx.conn;
    set_global_config(conn, "general.autonomy", &config.general.autonomy)?;
    set_global_config(conn, "general.auto_extract", &config.general.auto_extract.to_string())?;
    set_global_config(conn, "general.llm_provider", &config.general.llm_provider)?;
    set_global_config(conn, "general.llm_model", &config.general.llm_model)?;
    let aliases_json = serde_json::to_string(&config.tools.aliases)
        .map_err(|e| Error::Config(e.to_string()))?;
    set_global_config(conn, "tools.aliases", &aliases_json)?;
    Ok(())
}

pub fn run_wizard() -> Result<(), Error> {
    use dialoguer::{MultiSelect, Select, theme::ColorfulTheme};

    let theme = ColorfulTheme::default();

    println!("Redtrail setup wizard");
    println!("---------------------");

    let shell = detect_shell();
    println!("Detected shell: {}", shell);

    let found_tools = scan_tools();
    println!("Found {} pentesting tools on PATH", found_tools.len());

    let tool_labels: Vec<&str> = KNOWN_TOOLS.to_vec();
    let defaults: Vec<bool> = KNOWN_TOOLS
        .iter()
        .map(|t| found_tools.contains(&t.to_string()))
        .collect();

    let selected_indices = MultiSelect::with_theme(&theme)
        .with_prompt("Select tools to alias")
        .items(&tool_labels)
        .defaults(&defaults)
        .interact()
        .map_err(|e| Error::Config(e.to_string()))?;

    let selected_tools: Vec<String> = selected_indices
        .iter()
        .map(|&i| KNOWN_TOOLS[i].to_string())
        .collect();

    let providers = &["anthropic", "ollama", "skip"];
    let provider_idx = Select::with_theme(&theme)
        .with_prompt("LLM provider")
        .items(providers)
        .default(0)
        .interact()
        .map_err(|e| Error::Config(e.to_string()))?;

    let autonomy_opts = &["cautious", "balanced", "autonomous"];
    let autonomy_idx = Select::with_theme(&theme)
        .with_prompt("Autonomy mode")
        .items(autonomy_opts)
        .default(1)
        .interact()
        .map_err(|e| Error::Config(e.to_string()))?;

    let mut config = resolve::resolve_global()
        .ok()
        .and_then(|ctx| Config::resolved_global(&ctx.conn).ok())
        .unwrap_or_default();
    config.general.autonomy = autonomy_opts[autonomy_idx].to_string();
    config.tools.aliases = selected_tools;

    if provider_idx < 2 {
        config.general.auto_extract = true;
        config.general.llm_provider = providers[provider_idx].to_string();
    }

    save_config_to_db(&config)?;
    println!("Config saved to global database");
    Ok(())
}

pub fn run_status(json: bool) -> Result<(), Error> {
    let config = resolve::resolve_global()
        .ok()
        .and_then(|ctx| Config::resolved_global(&ctx.conn).ok())
        .unwrap_or_default();
    let db_path = resolve::global_db_path()?;
    let installed = db_path.exists();
    let shell = detect_shell();

    let cwd = std::env::current_dir()?;
    let active_workspace = resolve::resolve(&cwd)
        .ok()
        .map(|ctx| ctx.workspace_path.to_string_lossy().to_string());

    if json {
        let obj = serde_json::json!({
            "installed": installed,
            "shell": shell,
            "db_path": db_path.to_string_lossy(),
            "autonomy": config.general.autonomy,
            "auto_extract": config.general.auto_extract,
            "llm_provider": config.general.llm_provider,
            "tools": config.tools.aliases,
            "active_workspace": active_workspace,
        });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
        return Ok(());
    }

    println!("installed:        {}", installed);
    println!("shell:            {}", shell);
    println!("db_path:          {}", db_path.display());
    println!("autonomy:         {}", config.general.autonomy);
    println!("auto_extract:     {}", config.general.auto_extract);
    println!("tools:            {}", config.tools.aliases.join(", "));
    if let Some(ws) = active_workspace {
        println!("active_workspace: {}", ws);
    }
    Ok(())
}

pub fn run_aliases(args: AliasesArgs) -> Result<(), Error> {
    let mut config = resolve::resolve_global()
        .ok()
        .and_then(|ctx| Config::resolved_global(&ctx.conn).ok())
        .unwrap_or_default();

    if let Some(tool) = args.add {
        if !config.tools.aliases.contains(&tool) {
            config.tools.aliases.push(tool);
            save_config_to_db(&config)?;
            println!("added");
        } else {
            println!("already present");
        }
        return Ok(());
    }

    if let Some(tool) = args.remove {
        let before = config.tools.aliases.len();
        config.tools.aliases.retain(|t| t != &tool);
        if config.tools.aliases.len() < before {
            save_config_to_db(&config)?;
            println!("removed");
        } else {
            println!("not found");
        }
        return Ok(());
    }

    if args.list {
        for t in &config.tools.aliases {
            println!("{}", t);
        }
        return Ok(());
    }

    println!("Usage: rt setup aliases [--add <tool>] [--remove <tool>] [--list]");
    Ok(())
}
