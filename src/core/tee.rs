use crate::config::OnDetect;
use crate::core::db;
use crate::core::secrets::engine::{load_custom_patterns, redact_with_custom_patterns, CustomPattern};
use crate::error::Error;
use std::os::fd::AsRawFd;

/// Strip ANSI escape sequences from terminal output.
pub fn strip_ansi(input: &[u8]) -> String {
    let stripped = strip_ansi_escapes::strip(input);
    String::from_utf8_lossy(&stripped).to_string()
}

/// A PTY master/slave pair.
pub struct PtyPair {
    pub master_fd: std::os::fd::OwnedFd,
    pub slave_fd: std::os::fd::OwnedFd,
    pub slave_path: String,
}

/// Allocate a PTY pair using `openpty()`. Returns master fd, slave fd, and slave path.
///
/// Uses `openpty()` instead of the manual `posix_openpt`+`grantpt`+`unlockpt` sequence
/// because `openpty()` opens the slave fd immediately, which ensures the device node
/// exists in the container's devpts filesystem (critical for Docker compatibility).
///
/// If a controlling terminal exists (`/dev/tty`), the PTY inherits its window size.
pub fn allocate_pty_pair() -> Result<PtyPair, Error> {
    let winsize = get_tty_winsize();

    let result = nix::pty::openpty(winsize.as_ref(), None::<&nix::sys::termios::Termios>)
        .map_err(|e| Error::Pty(format!("openpty: {e}")))?;

    let slave_path = nix::unistd::ttyname(&result.slave)
        .map_err(|e| Error::Pty(format!("ttyname: {e}")))?
        .to_string_lossy()
        .to_string();

    Ok(PtyPair {
        master_fd: result.master,
        slave_fd: result.slave,
        slave_path,
    })
}

/// Get the current terminal window size, if available.
fn get_tty_winsize() -> Option<nix::pty::Winsize> {
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;

    let mut ws: nix::libc::winsize = unsafe { std::mem::zeroed() };
    let ret = unsafe { nix::libc::ioctl(tty.as_raw_fd(), nix::libc::TIOCGWINSZ, &mut ws) };
    if ret != 0 {
        return None;
    }

    Some(nix::pty::Winsize {
        ws_row: ws.ws_row,
        ws_col: ws.ws_col,
        ws_xpixel: ws.ws_xpixel,
        ws_ypixel: ws.ws_ypixel,
    })
}

/// Configuration for the tee relay loop.
pub struct TeeConfig {
    pub command_id: String,
    pub shell_pid: String,
    pub ctl_fifo: String,
    pub max_bytes: usize,
}

use std::sync::atomic::{AtomicBool, Ordering};

static FLUSH_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_sigusr1(_: nix::libc::c_int) {
    FLUSH_REQUESTED.store(true, Ordering::Relaxed);
}

/// Resolve DB path the same way cli.rs does.
fn resolve_db_path() -> Option<String> {
    std::env::var("REDTRAIL_DB").ok().or_else(|| {
        db::global_db_path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    })
}

/// Resolve config path the same way cli.rs does.
fn resolve_config_path() -> String {
    std::env::var("REDTRAIL_CONFIG").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.config/redtrail/config.yaml")
    })
}

/// Flush buffered output to the DB, applying secret redaction per on_detect mode.
/// Returns true if db_blocked was triggered (OnDetect::Block found secrets).
fn flush_to_db(
    db_conn: &rusqlite::Connection,
    command_id: &str,
    stdout_buf: &[u8],
    stderr_buf: &[u8],
    stdout_truncated: bool,
    stderr_truncated: bool,
    on_detect: OnDetect,
    custom_patterns: &[CustomPattern],
    warn_logged: &mut bool,
) -> bool {
    let stdout_clean = strip_ansi(stdout_buf);
    let stderr_clean = strip_ansi(stderr_buf);

    let stdout_opt = if stdout_clean.is_empty() {
        None
    } else {
        Some(stdout_clean.as_str())
    };
    let stderr_opt = if stderr_clean.is_empty() {
        None
    } else {
        Some(stderr_clean.as_str())
    };

    match on_detect {
        OnDetect::Redact => {
            let (stdout_redacted, _) =
                redact_with_custom_patterns(&stdout_clean, custom_patterns);
            let (stderr_redacted, _) =
                redact_with_custom_patterns(&stderr_clean, custom_patterns);
            let so = if stdout_redacted.is_empty() {
                None
            } else {
                Some(stdout_redacted.as_str())
            };
            let se = if stderr_redacted.is_empty() {
                None
            } else {
                Some(stderr_redacted.as_str())
            };
            let _ = db::update_command_output(
                db_conn,
                command_id,
                so,
                se,
                stdout_truncated,
                stderr_truncated,
            );
            false
        }
        OnDetect::Warn => {
            if !*warn_logged {
                let (_, stdout_labels) =
                    redact_with_custom_patterns(&stdout_clean, custom_patterns);
                let (_, stderr_labels) =
                    redact_with_custom_patterns(&stderr_clean, custom_patterns);
                if !stdout_labels.is_empty() || !stderr_labels.is_empty() {
                    eprintln!("[redtrail] WARN: secrets detected in output");
                    *warn_logged = true;
                }
            }
            let _ = db::update_command_output(
                db_conn,
                command_id,
                stdout_opt,
                stderr_opt,
                stdout_truncated,
                stderr_truncated,
            );
            false
        }
        OnDetect::Block => {
            let (_, stdout_labels) =
                redact_with_custom_patterns(&stdout_clean, custom_patterns);
            let (_, stderr_labels) =
                redact_with_custom_patterns(&stderr_clean, custom_patterns);
            if !stdout_labels.is_empty() || !stderr_labels.is_empty() {
                let _ = db::delete_command(db_conn, command_id);
                true
            } else {
                let _ = db::update_command_output(
                    db_conn,
                    command_id,
                    stdout_opt,
                    stderr_opt,
                    stdout_truncated,
                    stderr_truncated,
                );
                false
            }
        }
    }
}

/// Run the tee relay: allocate PTYs, write paths to FIFO, relay output, stream to DB.
pub fn run_tee(config: &TeeConfig) -> Result<(), Error> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use std::io::Write;

    let stdout_pty = allocate_pty_pair()?;
    let stderr_pty = allocate_pty_pair()?;

    // Write slave paths to FIFO — unblocks the shell's `read -t 1`
    {
        let mut fifo = std::fs::OpenOptions::new()
            .write(true)
            .open(&config.ctl_fifo)?;
        writeln!(fifo, "{} {}", stdout_pty.slave_path, stderr_pty.slave_path)?;
    }

    // Keep slave fds alive — dropping them before the shell opens its own causes
    // a race where the master sees EIO. Instead, we use SIGUSR1 from the shell's
    // precmd to signal "flush and exit."
    let _stdout_slave = stdout_pty.slave_fd;
    let _stderr_slave = stderr_pty.slave_fd;

    // Set up SIGUSR1 handler — precmd sends this to tell us to flush
    FLUSH_REQUESTED.store(false, Ordering::Relaxed);
    unsafe {
        nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGUSR1,
            nix::sys::signal::SigHandler::Handler(handle_sigusr1),
        )
        .ok();
    }

    // Open /dev/tty for relay output
    let mut tty = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")?;

    // Open DB connection — if this fails, tee still relays output to /dev/tty
    let db_conn_result = resolve_db_path().ok_or_else(|| {
        Error::Db("no DB path available".into())
    }).and_then(|path| db::open(&path));

    let db_available = db_conn_result.is_ok();
    let db_conn = db_conn_result.ok();

    // Load config for secret detection
    let app_config = crate::config::Config::load(&resolve_config_path()).unwrap_or_default();
    let on_detect = app_config.secrets.on_detect;
    let custom_patterns: Vec<CustomPattern> = app_config
        .secrets
        .patterns_file
        .as_deref()
        .map(load_custom_patterns)
        .unwrap_or_default();

    // Flush state
    let flush_interval = std::time::Duration::from_secs(1);
    let mut last_flush = std::time::Instant::now();
    let mut last_flush_stdout_len: usize = 0;
    let mut last_flush_stderr_len: usize = 0;
    let mut warn_logged = false;
    let mut db_blocked = false;

    let mut stdout_buf: Vec<u8> = Vec::new();
    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut stdout_truncated = false;
    let mut stderr_truncated = false;

    let stdout_fd = stdout_pty.master_fd.as_raw_fd();
    let stderr_fd = stderr_pty.master_fd.as_raw_fd();
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut read_buf = [0u8; 4096];

    // Instead of an inactivity timeout (which kills the relay for legitimate
    // long-running commands like webservers or interactive tools), we check
    // whether the parent shell is still alive. This catches orphaned tee
    // processes without breaking silent-but-running commands.
    let shell_pid: i32 = config.shell_pid.parse().unwrap_or(0);
    let orphan_check_interval = std::time::Duration::from_secs(5);
    let mut last_orphan_check = std::time::Instant::now();

    while !stdout_eof || !stderr_eof {
        let mut pollfds = Vec::new();
        if !stdout_eof {
            pollfds.push(PollFd::new(
                unsafe { std::os::fd::BorrowedFd::borrow_raw(stdout_fd) },
                PollFlags::POLLIN,
            ));
        }
        if !stderr_eof {
            pollfds.push(PollFd::new(
                unsafe { std::os::fd::BorrowedFd::borrow_raw(stderr_fd) },
                PollFlags::POLLIN,
            ));
        }

        match poll(&mut pollfds, PollTimeout::from(250u16)) {
            Ok(0) => {
                if FLUSH_REQUESTED.load(Ordering::Relaxed) {
                    break;
                }
                if last_orphan_check.elapsed() > orphan_check_interval {
                    last_orphan_check = std::time::Instant::now();
                    if shell_pid > 0 {
                        let alive = unsafe { nix::libc::kill(shell_pid, 0) };
                        if alive != 0 {
                            break;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => {
                if FLUSH_REQUESTED.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            }
            Err(_) => break,
        }

        let mut pf_idx = 0;

        if !stdout_eof {
            let revents = pollfds[pf_idx].revents().unwrap_or(PollFlags::empty());
            if revents.contains(PollFlags::POLLIN) {
                match nix::unistd::read(stdout_fd, &mut read_buf) {
                    Ok(0) => stdout_eof = true,
                    Ok(n) => {
                        let _ = tty.write_all(&read_buf[..n]);
                        if stdout_buf.len() < config.max_bytes {
                            let remaining = config.max_bytes - stdout_buf.len();
                            let take = n.min(remaining);
                            stdout_buf.extend_from_slice(&read_buf[..take]);
                            if n > remaining {
                                stdout_truncated = true;
                            }
                        }
                    }
                    Err(nix::errno::Errno::EIO) => stdout_eof = true,
                    Err(_) => stdout_eof = true,
                }
            }
            if revents.contains(PollFlags::POLLHUP) || revents.contains(PollFlags::POLLERR) {
                stdout_eof = true;
            }
            pf_idx += 1;
        }

        if !stderr_eof && pf_idx < pollfds.len() {
            let revents = pollfds[pf_idx].revents().unwrap_or(PollFlags::empty());
            if revents.contains(PollFlags::POLLIN) {
                match nix::unistd::read(stderr_fd, &mut read_buf) {
                    Ok(0) => stderr_eof = true,
                    Ok(n) => {
                        let _ = tty.write_all(&read_buf[..n]);
                        if stderr_buf.len() < config.max_bytes {
                            let remaining = config.max_bytes - stderr_buf.len();
                            let take = n.min(remaining);
                            stderr_buf.extend_from_slice(&read_buf[..take]);
                            if n > remaining {
                                stderr_truncated = true;
                            }
                        }
                    }
                    Err(nix::errno::Errno::EIO) => stderr_eof = true,
                    Err(_) => stderr_eof = true,
                }
            }
            if revents.contains(PollFlags::POLLHUP) || revents.contains(PollFlags::POLLERR) {
                stderr_eof = true;
            }
        }

        // Periodic flush to DB
        if db_available && !db_blocked && last_flush.elapsed() >= flush_interval {
            let stdout_has_new = stdout_buf.len() > last_flush_stdout_len;
            let stderr_has_new = stderr_buf.len() > last_flush_stderr_len;

            if stdout_has_new || stderr_has_new {
                if let Some(ref conn) = db_conn {
                    db_blocked = flush_to_db(
                        conn,
                        &config.command_id,
                        &stdout_buf,
                        &stderr_buf,
                        stdout_truncated,
                        stderr_truncated,
                        on_detect,
                        &custom_patterns,
                        &mut warn_logged,
                    );
                }
                last_flush_stdout_len = stdout_buf.len();
                last_flush_stderr_len = stderr_buf.len();
            }
            last_flush = std::time::Instant::now();
        }
    }

    // Final flush (unconditional, not gated by interval)
    if db_available && !db_blocked {
        let stdout_has_new = stdout_buf.len() > last_flush_stdout_len;
        let stderr_has_new = stderr_buf.len() > last_flush_stderr_len;

        if stdout_has_new || stderr_has_new {
            if let Some(ref conn) = db_conn {
                flush_to_db(
                    conn,
                    &config.command_id,
                    &stdout_buf,
                    &stderr_buf,
                    stdout_truncated,
                    stderr_truncated,
                    on_detect,
                    &custom_patterns,
                    &mut warn_logged,
                );
            }
        }
    }

    Ok(())
}
