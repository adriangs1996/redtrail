mod init;

use clap::{Parser, Subcommand};
use crate::error::Error;

#[derive(Parser)]
#[command(name = "rt", about = "Redtrail — pentesting workspace manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "general")]
        goal: String,
        #[arg(long)]
        scope: Option<String>,
    },
}

pub fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init { target, goal, scope }) => {
            init::run(target, goal, scope)
        }
        None => {
            println!("rt: redtrail. Use --help for usage.");
            Ok(())
        }
    }
}
