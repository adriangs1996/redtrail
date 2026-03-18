use clap::Subcommand;
use crate::db::Db;
use crate::error::Error;
use super::resolve_session;

#[derive(Subcommand)]
pub enum KbCommands {
    Hosts {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        host: Option<String>,
    },
    Ports {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        host: Option<String>,
    },
    Creds {
        #[arg(long)]
        json: bool,
    },
    Flags {
        #[arg(long)]
        json: bool,
    },
    Access {
        #[arg(long)]
        json: bool,
    },
    Notes {
        #[arg(long)]
        json: bool,
    },
    History {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    Search {
        query: String,
        #[arg(long)]
        json: bool,
    },
    AddHost {
        ip: String,
        #[arg(long)]
        os: Option<String>,
        #[arg(long)]
        hostname: Option<String>,
    },
    AddPort {
        ip: String,
        port: i64,
        #[arg(long)]
        protocol: Option<String>,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        version: Option<String>,
    },
    AddCred {
        username: String,
        #[arg(long)]
        pass: Option<String>,
        #[arg(long)]
        hash: Option<String>,
        #[arg(long)]
        service: Option<String>,
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    AddFlag {
        value: String,
        #[arg(long)]
        source: Option<String>,
    },
    AddAccess {
        host: String,
        user: String,
        level: String,
        #[arg(long)]
        method: Option<String>,
    },
    AddNote {
        text: String,
    },
    Extract {
        id: i64,
    },
}

pub fn run(cmd: KbCommands) -> Result<(), Error> {
    let (db, session_id) = resolve_session()?;

    match cmd {
        KbCommands::Hosts { json, host } => {
            let rows = db.list_hosts(&session_id)?;
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
                        println!("{:<18} {:<20} {:<15} {}",
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
            let rows = db.list_ports(&session_id, host.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no ports");
            } else {
                println!("{:<18} {:<8} {:<8} {:<15} VERSION", "IP", "PORT", "PROTO", "SERVICE");
                for r in &rows {
                    println!("{:<18} {:<8} {:<8} {:<15} {}",
                        r["ip"].as_str().unwrap_or(""),
                        r["port"].as_i64().map(|p| p.to_string()).unwrap_or_default(),
                        r["protocol"].as_str().unwrap_or(""),
                        r["service"].as_str().unwrap_or("-"),
                        r["version"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Creds { json } => {
            let rows = db.list_credentials(&session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no credentials");
            } else {
                println!("{:<20} {:<20} {:<15} {:<15} SOURCE", "USERNAME", "PASSWORD", "SERVICE", "HOST");
                for r in &rows {
                    println!("{:<20} {:<20} {:<15} {:<15} {}",
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
            let rows = db.list_flags(&session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no flags");
            } else {
                println!("{:<40} {:<20} CAPTURED_AT", "VALUE", "SOURCE");
                for r in &rows {
                    println!("{:<40} {:<20} {}",
                        r["value"].as_str().unwrap_or(""),
                        r["source"].as_str().unwrap_or("-"),
                        r["captured_at"].as_str().unwrap_or(""),
                    );
                }
            }
        }
        KbCommands::Access { json } => {
            let rows = db.list_access(&session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no access entries");
            } else {
                println!("{:<18} {:<20} {:<12} METHOD", "HOST", "USER", "LEVEL");
                for r in &rows {
                    println!("{:<18} {:<20} {:<12} {}",
                        r["host"].as_str().unwrap_or(""),
                        r["user"].as_str().unwrap_or(""),
                        r["level"].as_str().unwrap_or(""),
                        r["method"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Notes { json } => {
            let rows = db.list_notes(&session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no notes");
            } else {
                for r in &rows {
                    println!("[{}] {}", r["created_at"].as_str().unwrap_or(""), r["text"].as_str().unwrap_or(""));
                }
            }
        }
        KbCommands::History { json, limit } => {
            let rows = db.list_history(&session_id, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no history");
            } else {
                for r in &rows {
                    println!("[{}] {}", r["started_at"].as_str().unwrap_or(""), r["command"].as_str().unwrap_or(""));
                }
            }
        }
        KbCommands::Search { query, json } => {
            let rows = db.search(&session_id, &query)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no results");
            } else {
                for r in &rows {
                    println!("[{}] {}", r["kind"].as_str().unwrap_or(""), r["value"].as_str().unwrap_or(""));
                }
            }
        }
        KbCommands::AddHost { ip, os, hostname } => {
            let id = db.add_host(&session_id, &ip, os.as_deref(), hostname.as_deref())?;
            println!("host added (id={id}): {ip}");
        }
        KbCommands::AddPort { ip, port, protocol, service, version } => {
            let id = db.add_port(&session_id, &ip, port, protocol.as_deref(), service.as_deref(), version.as_deref())?;
            println!("port added (id={id}): {ip}:{port}");
        }
        KbCommands::AddCred { username, pass, hash, service, host, source } => {
            let id = db.add_credential(&session_id, &username, pass.as_deref(), hash.as_deref(), service.as_deref(), host.as_deref(), source.as_deref())?;
            println!("credential added (id={id}): {username}");
        }
        KbCommands::AddFlag { value, source } => {
            let id = db.add_flag(&session_id, &value, source.as_deref())?;
            println!("flag added (id={id}): {value}");
        }
        KbCommands::AddAccess { host, user, level, method } => {
            let id = db.add_access(&session_id, &host, &user, &level, method.as_deref())?;
            println!("access added (id={id}): {user}@{host} ({level})");
        }
        KbCommands::AddNote { text } => {
            let id = db.add_note(&session_id, &text)?;
            println!("note added (id={id})");
        }
        KbCommands::Extract { id } => {
            let config = crate::config::Config::resolved(&std::env::current_dir()?)?;
            crate::extraction::extract_sync(&db, &session_id, id, &config)?;
            println!("extraction complete for command {id}");
        }
    }

    Ok(())
}
