use crate::db::{self, KnowledgeBase, SessionOps};
use crate::error::Error;
use clap::Subcommand;
use rusqlite::Connection;

#[derive(Subcommand)]
pub enum SessionCommands {
    #[command(about = "List all sessions in the workspace")]
    List {
        #[arg(long, help = "Show sessions across all workspaces")]
        all: bool,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Show details of the active session")]
    Active {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Export full session data (hosts, ports, creds, flags, notes)")]
    Export {
        #[arg(long, default_value = "json", help = "Output format")]
        format: String,
    },
    #[command(about = "Create a new session for this workspace")]
    New {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "general")]
        goal: String,
        #[arg(long)]
        scope: Option<String>,
    },
    #[command(about = "Activate an archived session")]
    Activate {
        #[arg(help = "Session name or ID")]
        name_or_id: String,
    },
}

pub fn run_with_conn(conn: &Connection, cmd: SessionCommands) -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let workspace_path = cwd.to_string_lossy().to_string();

    match cmd {
        SessionCommands::List { all, json } => {
            let rows = if all {
                db::session::list_all_sessions(conn)?
            } else {
                db::session::list_sessions(conn, &workspace_path)?
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                let active_id = db::session::active_session_id(conn, &workspace_path)
                    .ok();

                if all {
                    println!("{:<3} {:<36} {:<20} {:<18} {}", "", "ID", "NAME", "TARGET", "WORKSPACE");
                } else {
                    println!("{:<3} {:<36} {:<20} {:<18} {}", "", "ID", "NAME", "TARGET", "PHASE");
                }
                for r in &rows {
                    let marker = if active_id.as_deref() == r["id"].as_str() { "*" } else { " " };
                    let id = r["id"].as_str().unwrap_or("");
                    let name = r["name"].as_str().unwrap_or("");
                    let target = r["target"].as_str().unwrap_or("-");
                    if all {
                        let wp = r["workspace_path"].as_str().unwrap_or("");
                        println!("{:<3} {:<36} {:<20} {:<18} {}", marker, id, name, target, wp);
                    } else {
                        let phase = r["phase"].as_str().unwrap_or("");
                        println!("{:<3} {:<36} {:<20} {:<18} {}", marker, id, name, target, phase);
                    }
                }
                if rows.is_empty() {
                    println!("(no sessions)");
                }
            }
            Ok(())
        }

        SessionCommands::Active { json } => {
            let session_id = db::session::active_session_id(conn, &workspace_path)?;
            let row = db::session::get_session(conn, &session_id)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&row).unwrap());
            } else {
                println!("id:         {}", row["id"].as_str().unwrap_or(""));
                println!("name:       {}", row["name"].as_str().unwrap_or(""));
                println!("target:     {}", row["target"].as_str().unwrap_or("-"));
                println!("scope:      {}", row["scope"].as_str().unwrap_or("(unrestricted)"));
                println!("goal:       {}", row["goal"].as_str().unwrap_or(""));
                println!("phase:      {}", row["phase"].as_str().unwrap_or(""));
                println!("workspace:  {}", row["workspace_path"].as_str().unwrap_or(""));
                println!("created:    {}", row["created_at"].as_str().unwrap_or(""));
            }
            Ok(())
        }

        SessionCommands::Export { format: _ } => {
            let session_id = db::session::active_session_id(conn, &workspace_path)?;
            let db_path = crate::resolve::global_db_path()?;
            let db = db::open(db_path.to_str().unwrap())?;
            let session_row = db.get_session(&session_id)?;
            let ws = session_row["workspace_path"].as_str().unwrap_or("");

            let export = serde_json::json!({
                "workspace": ws,
                "session": session_row,
                "hosts": db.list_hosts(&session_id)?,
                "ports": db.list_ports(&session_id, None)?,
                "credentials": db.list_credentials(&session_id)?,
                "flags": db.list_flags(&session_id)?,
                "access": db.list_access(&session_id)?,
                "notes": db.list_notes(&session_id)?,
            });

            println!("{}", serde_json::to_string_pretty(&export).unwrap());
            Ok(())
        }

        SessionCommands::New { target, goal, scope } => {
            db::session::deactivate_session(conn, &workspace_path)?;

            let base = cwd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("default")
                .to_string();
            let existing = db::session::list_sessions(conn, &workspace_path)?;
            let count = existing.len();
            let session_name = if count == 0 {
                base.clone()
            } else {
                format!("{base}-{}", count + 1)
            };
            let session_id = uuid::Uuid::new_v4().to_string();

            db::session::create_session(
                conn,
                &session_id,
                &session_name,
                &workspace_path,
                target.as_deref(),
                scope.as_deref(),
                &goal,
            )?;

            println!("session created: {session_name}");
            println!("  id: {session_id}");
            if let Some(ref t) = target {
                println!("  target: {t}");
            }
            println!("previous session archived");
            Ok(())
        }

        SessionCommands::Activate { name_or_id } => {
            let row = db::session::find_session_by_name_or_id(conn, &name_or_id, &workspace_path)?;

            let target_id = row["id"].as_str().unwrap().to_string();

            let active_check = db::session::active_session_id(conn, &workspace_path);
            if let Ok(ref current) = active_check {
                if current == &target_id {
                    println!("session '{}' is already active", name_or_id);
                    return Ok(());
                }
            }

            db::session::deactivate_session(conn, &workspace_path)?;
            db::session::activate_session(conn, &target_id)?;

            println!("activated session: {} ({})", row["name"].as_str().unwrap_or(""), target_id);
            Ok(())
        }
    }
}

