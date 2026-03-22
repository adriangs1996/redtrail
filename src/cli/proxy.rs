use crate::error::Error;
use crate::resolve;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::time::Instant;

pub fn run(args: &[String]) -> Result<(), Error> {
    if args.is_empty() {
        return Ok(());
    }

    let cmd_str = args.join(" ");
    let tool = args.first().map(|s| s.as_str());

    let cwd = std::env::current_dir()?;

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

    let mut cmd = CommandBuilder::new(&args[0]);
    for arg in &args[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

    let start = Instant::now();
    let mut output = Vec::new();
    let mut buf = [0u8; 4096];

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                std::io::stdout().write_all(&buf[..n])?;
                std::io::stdout().flush()?;
                output.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Other => break,
            Err(e) => return Err(Error::Io(e)),
        }
    }

    let status = child
        .wait()
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    let exit_code = status.exit_code() as i32;
    let duration_ms = start.elapsed().as_millis() as i64;
    let output_str = String::from_utf8_lossy(&output).to_string();

    if let Ok(ctx) = resolve::resolve(&cwd) {
        let config = ctx.config.clone();
        let conn = ctx.conn;
        let session_id = ctx.session_id;
        if let Ok(result) = crate::pipeline::process_command(
            &conn,
            &session_id,
            &cmd_str,
            exit_code,
            duration_ms,
            &output_str,
            tool,
        ) {
            for flag in &result.flags_found {
                eprintln!("[rt] flag captured: {flag}");
            }
            for warn in &result.scope_warnings {
                eprintln!("[rt] warning: {warn}");
            }
            if config.general.auto_extract {
                crate::spawn::spawn_extraction(result.command_id);
            }
        }
    }

    std::process::exit(exit_code);
}
