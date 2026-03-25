mod cli;
mod cmd;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("[rt] error: {e}");
        std::process::exit(1);
    }
}
