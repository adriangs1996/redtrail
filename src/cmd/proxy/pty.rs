use redtrail::error::Error;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::time::Instant;

pub struct PtyResult {
    pub exit_code: i32,
    pub duration_ms: i64,
    pub output: String,
}

pub fn spawn_and_capture(args: &[String]) -> Result<PtyResult, Error> {
    let cwd = std::env::current_dir()?;
    let (rows, cols) = terminal_size().unwrap_or((24, 80));

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
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

    Ok(PtyResult { exit_code, duration_ms, output: output_str })
}

fn terminal_size() -> Option<(u16, u16)> {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 {
            Some((ws.ws_row, ws.ws_col))
        } else {
            None
        }
    }
}
