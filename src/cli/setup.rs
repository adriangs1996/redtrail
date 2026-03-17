use clap::{Args, Subcommand};
use crate::error::Error;
use crate::config::Config;
use crate::workspace;

const KNOWN_TOOLS: &[&str] = &[
    "nmap", "gobuster", "feroxbuster", "ffuf", "dirb", "nikto", "sqlmap", "hydra",
    "crackmapexec", "whatweb", "nuclei", "john", "hashcat", "curl", "wget", "ssh",
    "scp", "nc", "netcat", "enum4linux", "responder", "wfuzz",
];

#[derive(Subcommand)]
pub enum SetupCommands {
    Status {
        #[arg(long)]
        json: bool,
    },
    Aliases(AliasesArgs),
}

#[derive(Args)]
pub struct AliasesArgs {
    #[arg(long)]
    pub add: Option<String>,
    #[arg(long)]
    pub remove: Option<String>,
    #[arg(long)]
    pub list: bool,
}

fn global_config_path() -> Result<std::path::PathBuf, Error> {
    Ok(dirs::home_dir()
        .ok_or_else(|| Error::Config("cannot determine home directory".to_string()))?
        .join(".redtrail/config.toml"))
}

fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.split('/').last().map(String::from))
        .unwrap_or_else(|| "unknown".to_string())
}

fn scan_tools() -> Vec<String> {
    KNOWN_TOOLS.iter()
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

fn write_global_config(config: &Config) -> Result<std::path::PathBuf, Error> {
    let path = global_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| Error::Config(e.to_string()))?;
    std::fs::write(&path, toml_str)?;
    Ok(path)
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
    let defaults: Vec<bool> = KNOWN_TOOLS.iter()
        .map(|t| found_tools.contains(&t.to_string()))
        .collect();

    let selected_indices = MultiSelect::with_theme(&theme)
        .with_prompt("Select tools to alias")
        .items(&tool_labels)
        .defaults(&defaults)
        .interact()
        .map_err(|e| Error::Config(e.to_string()))?;

    let selected_tools: Vec<String> = selected_indices.iter()
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

    let mut config = Config::load_global().unwrap_or_default();
    config.general.autonomy = autonomy_opts[autonomy_idx].to_string();
    config.tools.aliases = selected_tools;

    if provider_idx < 2 {
        config.general.auto_extract = true;
    }

    let path = write_global_config(&config)?;
    println!("Config written to {}", path.display());
    Ok(())
}

pub fn run_status(json: bool) -> Result<(), Error> {
    let config = Config::load_global().unwrap_or_default();
    let config_path = global_config_path()?;
    let installed = config_path.exists();
    let shell = detect_shell();

    let cwd = std::env::current_dir()?;
    let active_workspace = workspace::find_workspace(&cwd)
        .map(|p| p.to_string_lossy().to_string());

    if json {
        let obj = serde_json::json!({
            "installed": installed,
            "shell": shell,
            "config_path": config_path.to_string_lossy(),
            "autonomy": config.general.autonomy,
            "auto_extract": config.general.auto_extract,
            "llm_provider": "anthropic",
            "tools": config.tools.aliases,
            "active_workspace": active_workspace,
        });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
        return Ok(());
    }

    println!("installed:        {}", installed);
    println!("shell:            {}", shell);
    println!("config_path:      {}", config_path.display());
    println!("autonomy:         {}", config.general.autonomy);
    println!("auto_extract:     {}", config.general.auto_extract);
    println!("tools:            {}", config.tools.aliases.join(", "));
    if let Some(ws) = active_workspace {
        println!("active_workspace: {}", ws);
    }
    Ok(())
}

pub fn run_aliases(args: AliasesArgs) -> Result<(), Error> {
    let config_path = global_config_path()?;

    let mut config = Config::load_global().unwrap_or_default();

    if let Some(tool) = args.add {
        if !config.tools.aliases.contains(&tool) {
            config.tools.aliases.push(tool);
            write_global_config(&config)?;
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
            write_global_config(&config)?;
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
    let _ = config_path;
    Ok(())
}
