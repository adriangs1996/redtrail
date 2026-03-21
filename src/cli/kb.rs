use crate::db::KnowledgeBase;
use crate::error::Error;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum KbCommands {
    #[command(about = "List discovered hosts")]
    Hosts {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Filter by IP address")]
        host: Option<String>,
    },
    #[command(about = "List discovered ports")]
    Ports {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Filter by host IP")]
        host: Option<String>,
    },
    #[command(about = "List harvested credentials")]
    Creds {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "List captured flags")]
    Flags {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "List access entries (shells, sessions, privilege levels)")]
    Access {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "List operator notes")]
    Notes {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "List discovered web paths and directories")]
    Paths {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Filter by host IP")]
        host: Option<String>,
    },
    #[command(about = "List discovered vulnerabilities")]
    Vulns {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Filter by host IP")]
        host: Option<String>,
        #[arg(long, help = "Filter by severity (critical, high, medium, low, info)")]
        severity: Option<String>,
    },
    #[command(about = "List command execution history")]
    History {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, default_value = "50", help = "Max number of entries to show")]
        limit: usize,
    },
    #[command(about = "Full-text search across the knowledge base")]
    Search {
        #[arg(help = "Search query string")]
        query: String,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

pub fn run(
    db: &impl KnowledgeBase,
    session_id: &str,
    cmd: KbCommands,
) -> Result<(), Error> {
    match cmd {
        KbCommands::Hosts { json, host } => {
            let rows = db.list_hosts(session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                let filtered: Vec<_> = if let Some(ref ip) = host {
                    rows.iter().filter(|r| r["ip"] == *ip).collect()
                } else {
                    rows.iter().collect()
                };
                if filtered.is_empty() {
                    println!("no hosts");
                } else {
                    println!("{:<18} {:<20} {:<15} STATUS", "IP", "HOSTNAME", "OS");
                    for r in filtered {
                        println!(
                            "{:<18} {:<20} {:<15} {}",
                            r["ip"].as_str().unwrap_or(""),
                            r["hostname"].as_str().unwrap_or("-"),
                            r["os"].as_str().unwrap_or("-"),
                            r["status"].as_str().unwrap_or(""),
                        );
                    }
                }
            }
        }
        KbCommands::Ports { json, host } => {
            let rows = db.list_ports(session_id, host.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no ports");
            } else {
                println!(
                    "{:<18} {:<8} {:<8} {:<15} VERSION",
                    "IP", "PORT", "PROTO", "SERVICE"
                );
                for r in &rows {
                    println!(
                        "{:<18} {:<8} {:<8} {:<15} {}",
                        r["ip"].as_str().unwrap_or(""),
                        r["port"]
                            .as_i64()
                            .map(|p| p.to_string())
                            .unwrap_or_default(),
                        r["protocol"].as_str().unwrap_or(""),
                        r["service"].as_str().unwrap_or("-"),
                        r["version"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Creds { json } => {
            let rows = db.list_credentials(session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no credentials");
            } else {
                println!(
                    "{:<20} {:<20} {:<15} {:<15} SOURCE",
                    "USERNAME", "PASSWORD", "SERVICE", "HOST"
                );
                for r in &rows {
                    println!(
                        "{:<20} {:<20} {:<15} {:<15} {}",
                        r["username"].as_str().unwrap_or(""),
                        r["password"].as_str().unwrap_or("-"),
                        r["service"].as_str().unwrap_or("-"),
                        r["host"].as_str().unwrap_or("-"),
                        r["source"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Flags { json } => {
            let rows = db.list_flags(session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no flags");
            } else {
                println!("{:<40} {:<20} CAPTURED_AT", "VALUE", "SOURCE");
                for r in &rows {
                    println!(
                        "{:<40} {:<20} {}",
                        r["value"].as_str().unwrap_or(""),
                        r["source"].as_str().unwrap_or("-"),
                        r["captured_at"].as_str().unwrap_or(""),
                    );
                }
            }
        }
        KbCommands::Access { json } => {
            let rows = db.list_access(session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no access entries");
            } else {
                println!("{:<18} {:<20} {:<12} METHOD", "HOST", "USER", "LEVEL");
                for r in &rows {
                    println!(
                        "{:<18} {:<20} {:<12} {}",
                        r["host"].as_str().unwrap_or(""),
                        r["user"].as_str().unwrap_or(""),
                        r["level"].as_str().unwrap_or(""),
                        r["method"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Notes { json } => {
            let rows = db.list_notes(session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no notes");
            } else {
                for r in &rows {
                    println!(
                        "[{}] {}",
                        r["created_at"].as_str().unwrap_or(""),
                        r["text"].as_str().unwrap_or("")
                    );
                }
            }
        }
        KbCommands::Paths { json, host } => {
            let rows = db.list_web_paths(session_id, host.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no paths");
            } else {
                println!(
                    "{:<18} {:<6} {:<8} {:<30} {:<6} {:<8} TYPE",
                    "IP", "PORT", "SCHEME", "PATH", "STATUS", "LENGTH"
                );
                for r in &rows {
                    println!(
                        "{:<18} {:<6} {:<8} {:<30} {:<6} {:<8} {}",
                        r["ip"].as_str().unwrap_or(""),
                        r["port"].as_i64().map(|p| p.to_string()).unwrap_or_default(),
                        r["scheme"].as_str().unwrap_or("http"),
                        r["path"].as_str().unwrap_or(""),
                        r["status_code"].as_i64().map(|s| s.to_string()).unwrap_or("-".into()),
                        r["content_length"].as_i64().map(|l| l.to_string()).unwrap_or("-".into()),
                        r["content_type"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Vulns { json, host, severity } => {
            let rows = db.list_vulns(session_id, host.as_deref(), severity.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no vulns");
            } else {
                println!(
                    "{:<18} {:<6} {:<30} {:<10} {:<18} URL",
                    "IP", "PORT", "NAME", "SEVERITY", "CVE"
                );
                for r in &rows {
                    println!(
                        "{:<18} {:<6} {:<30} {:<10} {:<18} {}",
                        r["ip"].as_str().unwrap_or(""),
                        r["port"].as_i64().map(|p| p.to_string()).unwrap_or_default(),
                        r["name"].as_str().unwrap_or(""),
                        r["severity"].as_str().unwrap_or("-"),
                        r["cve"].as_str().unwrap_or("-"),
                        r["url"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::History { json, limit } => {
            let rows = db.list_history(session_id, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no history");
            } else {
                for r in &rows {
                    println!(
                        "[{}] {}",
                        r["started_at"].as_str().unwrap_or(""),
                        r["command"].as_str().unwrap_or("")
                    );
                }
            }
        }
        KbCommands::Search { query, json } => {
            let rows = db.search(session_id, &query)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no results");
            } else {
                for r in &rows {
                    println!(
                        "[{}] {}",
                        r["kind"].as_str().unwrap_or(""),
                        r["value"].as_str().unwrap_or("")
                    );
                }
            }
        }
    }

    Ok(())
}
