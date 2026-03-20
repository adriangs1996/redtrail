use crate::db::SessionOps;
use crate::error::Error;
use crate::net;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ScopeCommands {
    #[command(
        about = "Check if an IP is within the session's defined scope (exit 0 = in, exit 1 = out)"
    )]
    Check {
        #[arg(help = "IP address to check")]
        ip: String,
    },
}

pub fn run(db: &impl SessionOps, session_id: &str, cmd: ScopeCommands) -> Result<(), Error> {
    match cmd {
        ScopeCommands::Check { ip } => {
            let scope = db.load_scope(session_id)?;

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
