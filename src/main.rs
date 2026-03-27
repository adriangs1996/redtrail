mod cli;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("[redtrail] error: {e}");
        std::process::exit(1);
    }
}
