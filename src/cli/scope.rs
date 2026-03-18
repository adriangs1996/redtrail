use clap::Subcommand;
use crate::db::Db;
use crate::error::Error;
use crate::net;
use super::resolve_session;

#[derive(Subcommand)]
pub enum ScopeCommands {
    Check {
        ip: String,
    },
}

pub fn run(cmd: ScopeCommands) -> Result<(), Error> {
    match cmd {
        ScopeCommands::Check { ip } => {
            let (db, session_id) = resolve_session()?;
            let scope = db.load_scope(&session_id)?;

            let in_scope = match scope.as_deref() {
                None | Some("") => true,
                Some(s) => net::ip_in_scope(&ip, s),
            };

            if in_scope {
                println!("in-scope");
                Ok(())
            } else {
                println!("out-of-scope");
                std::process::exit(1);
            }
        }
    }
}
