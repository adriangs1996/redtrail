use crate::error::Error;
use std::os::fd::AsRawFd;
use std::path::Path;

/// Header metadata written to stdout/stderr capture temp files.
pub struct TempFileHeader {
    pub ts_start: i64,
    pub ts_end: i64,
    pub truncated: bool,
}

/// Write a capture temp file with header and content. File is created with mode 0600.
pub fn write_capture_file(
    path: &Path,
    header: &TempFileHeader,
    content: &str,
) -> Result<(), Error> {
    use std::io::Write;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        write!(
            f,
            "ts_start:{}\nts_end:{}\ntruncated:{}\n\n{}",
            header.ts_start, header.ts_end, header.truncated, content
        )?;
    }

    #[cfg(not(unix))]
    {
        let mut f = std::fs::File::create(path)?;
        write!(
            f,
            "ts_start:{}\nts_end:{}\ntruncated:{}\n\n{}",
            header.ts_start, header.ts_end, header.truncated, content
        )?;
    }

    Ok(())
}

/// Read a capture temp file. Returns None if the file doesn't exist.
/// Returns (header, content) on success.
pub fn read_capture_file(path: &Path) -> Option<(TempFileHeader, String)> {
    let data = std::fs::read_to_string(path).ok()?;

    let mut ts_start: i64 = 0;
    let mut ts_end: i64 = 0;
    let mut truncated = false;

    let content_start = data.find("\n\n").map(|i| i + 2).unwrap_or(data.len());
    let header_section = &data[..content_start.saturating_sub(2).min(data.len())];

    for line in header_section.lines() {
        if let Some(val) = line.strip_prefix("ts_start:") {
            ts_start = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("ts_end:") {
            ts_end = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("truncated:") {
            truncated = val == "true";
        }
    }

    let content = if content_start < data.len() {
        data[content_start..].to_string()
    } else {
        String::new()
    };

    Some((
        TempFileHeader {
            ts_start,
            ts_end,
            truncated,
        },
        content,
    ))
}

/// Strip ANSI escape sequences from terminal output.
pub fn strip_ansi(input: &[u8]) -> String {
    let stripped = strip_ansi_escapes::strip(input);
    String::from_utf8_lossy(&stripped).to_string()
}

/// A PTY master/slave pair.
pub struct PtyPair {
    pub master_fd: nix::pty::PtyMaster,
    pub slave_path: String,
}

/// Allocate a PTY pair. Returns the master fd and the slave device path.
pub fn allocate_pty_pair() -> Result<PtyPair, Error> {
    use nix::fcntl::OFlag;
    use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};

    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)
        .map_err(|e| Error::Pty(format!("posix_openpt: {e}")))?;

    grantpt(&master).map_err(|e| Error::Pty(format!("grantpt: {e}")))?;
    unlockpt(&master).map_err(|e| Error::Pty(format!("unlockpt: {e}")))?;

    // Safety: ptsname returns a pointer to a static buffer. We call it only from one
    // thread at a time (each tee process is single-threaded). On Linux, ptsname_r would
    // be preferred but it's behind a cfg gate.
    let slave_path =
        unsafe { ptsname(&master) }.map_err(|e| Error::Pty(format!("ptsname: {e}")))?;

    Ok(PtyPair {
        master_fd: master, // PtyMaster wraps OwnedFd, implements AsRawFd
        slave_path,
    })
}

/// Set the window size on a PTY slave fd from the current /dev/tty dimensions.
pub fn init_pty_winsize(slave_path: &str) -> Result<(), Error> {
    use std::fs::OpenOptions;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|e| Error::Pty(format!("open /dev/tty: {e}")))?;

    let mut ws: nix::libc::winsize = unsafe { std::mem::zeroed() };
    let ret = unsafe { nix::libc::ioctl(tty.as_raw_fd(), nix::libc::TIOCGWINSZ, &mut ws) };
    if ret != 0 {
        return Err(Error::Pty("TIOCGWINSZ failed".into()));
    }

    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOCTTY)
        .open(slave_path)
        .map_err(|e| Error::Pty(format!("open slave: {e}")))?;

    let ret =
        unsafe { nix::libc::ioctl(slave.as_raw_fd(), nix::libc::TIOCSWINSZ, &ws) };
    if ret != 0 {
        return Err(Error::Pty("TIOCSWINSZ failed".into()));
    }

    Ok(())
}

/// Configuration for the tee relay loop.
pub struct TeeConfig {
    pub session_id: String,
    pub shell_pid: String,
    pub ctl_fifo: String,
    pub max_bytes: usize,
}

/// Run the tee relay: allocate PTYs, write paths to FIFO, relay output, write temp files on EOF.
pub fn run_tee(config: &TeeConfig) -> Result<(), Error> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use std::io::Write;

    let stdout_pty = allocate_pty_pair()?;
    let stderr_pty = allocate_pty_pair()?;

    // Initialize window size (best-effort)
    let _ = init_pty_winsize(&stdout_pty.slave_path);
    let _ = init_pty_winsize(&stderr_pty.slave_path);

    let ts_start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Write slave paths to FIFO — unblocks the shell's `read -t 1`
    {
        let mut fifo = std::fs::OpenOptions::new()
            .write(true)
            .open(&config.ctl_fifo)?;
        writeln!(fifo, "{} {}", stdout_pty.slave_path, stderr_pty.slave_path)?;
    }

    // Open /dev/tty for relay output
    let mut tty = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")?;

    let mut stdout_buf: Vec<u8> = Vec::new();
    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut stdout_truncated = false;
    let mut stderr_truncated = false;

    let stdout_fd = stdout_pty.master_fd.as_raw_fd();
    let stderr_fd = stderr_pty.master_fd.as_raw_fd();
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut read_buf = [0u8; 4096];

    let inactivity_timeout = std::time::Duration::from_secs(300);
    let mut last_activity = std::time::Instant::now();

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

        match poll(&mut pollfds, PollTimeout::from(1000u16)) {
            Ok(0) => {
                if last_activity.elapsed() > inactivity_timeout {
                    break;
                }
                continue;
            }
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => break,
        }

        let mut pf_idx = 0;

        if !stdout_eof {
            let revents = pollfds[pf_idx].revents().unwrap_or(PollFlags::empty());
            if revents.contains(PollFlags::POLLIN) {
                match nix::unistd::read(stdout_fd, &mut read_buf) {
                    Ok(0) => stdout_eof = true,
                    Ok(n) => {
                        last_activity = std::time::Instant::now();
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
                        last_activity = std::time::Instant::now();
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
    }

    let ts_end = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Strip ANSI only — secret redaction handled by redtrail capture
    let stdout_clean = strip_ansi(&stdout_buf);
    let stderr_clean = strip_ansi(&stderr_buf);

    let out_path = format!("/tmp/rt-out-{}", config.shell_pid);
    let err_path = format!("/tmp/rt-err-{}", config.shell_pid);

    if !stdout_clean.is_empty() {
        write_capture_file(
            Path::new(&out_path),
            &TempFileHeader { ts_start, ts_end, truncated: stdout_truncated },
            &stdout_clean,
        )?;
    }

    if !stderr_clean.is_empty() {
        write_capture_file(
            Path::new(&err_path),
            &TempFileHeader { ts_start, ts_end, truncated: stderr_truncated },
            &stderr_clean,
        )?;
    }

    Ok(())
}
