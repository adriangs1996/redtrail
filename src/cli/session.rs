use clap::Subcommand;
use crate::db::Db;
use crate::error::Error;
use crate::workspace;
use super::resolve_session;

#[derive(Subcommand)]
pub enum SessionCommands {
    List {
        #[arg(long)]
        json: bool,
    },
    Active {
        #[arg(long)]
        json: bool,
    },
    Export {
        #[arg(long, default_value = "json")]
        format: String,
    },
}

pub fn run(cmd: SessionCommands) -> Result<(), Error> {
    match cmd {
        SessionCommands::List { json } => {
            let (db, session_id) = resolve_session()?;
            let row = db.get_session(&session_id)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!([row])).unwrap());
            } else {
                println!("{:<36} {:<20} {:<18} PHASE", "ID", "NAME", "TARGET");
                println!("{:<36} {:<20} {:<18} {}",
                    row["id"].as_str().unwrap_or(""),
                    row["name"].as_str().unwrap_or(""),
                    row["target"].as_str().unwrap_or("-"),
                    row["phase"].as_str().unwrap_or(""),
                );
            }
            Ok(())
        }

        SessionCommands::Active { json } => {
            let (db, session_id) = resolve_session()?;
            let row = db.get_session(&session_id)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&row).unwrap());
            } else {
                println!("id:         {}", row["id"].as_str().unwrap_or(""));
                println!("name:       {}", row["name"].as_str().unwrap_or(""));
                println!("target:     {}", row["target"].as_str().unwrap_or("-"));
                println!("scope:      {}", row["scope"].as_str().unwrap_or("(unrestricted)"));
                println!("goal:       {}", row["goal"].as_str().unwrap_or(""));
                println!("phase:      {}", row["phase"].as_str().unwrap_or(""));
                println!("autonomy:   {}", row["autonomy"].as_str().unwrap_or(""));
                println!("created:    {}", row["created_at"].as_str().unwrap_or(""));
            }
            Ok(())
        }

        SessionCommands::Export { format: _ } => {
            let (db, session_id) = resolve_session()?;
            let cwd = std::env::current_dir()?;
            let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;

            let session_row = db.get_session(&session_id)?;

            let export = serde_json::json!({
                "workspace": ws.to_string_lossy(),
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
    }
}
