mod cli;
mod config;
mod db;
mod error;
mod extraction;
mod net;
mod pipeline;
mod skill_loader;
mod workspace;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
