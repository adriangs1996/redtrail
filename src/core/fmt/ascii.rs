use std::io::IsTerminal;

// ANSI color codes
pub const GREEN: &str = "\x1b[32m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";
pub const YELLOW: &str = "\x1b[33m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

/// Returns true if stdout supports color output.
/// Respects NO_COLOR env var (https://no-color.org/) and detects pipes.
pub fn colors_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Returns the effective terminal width, defaulting to 100.
pub fn terminal_width() -> usize {
    if let Ok(cols) = std::env::var("COLUMNS")
        && let Ok(w) = cols.parse::<usize>()
        && w > 40
    {
        return w;
    }
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdout().as_raw_fd();
        let mut ws: nix::libc::winsize = unsafe { std::mem::zeroed() };
        let ret = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCGWINSZ, &mut ws) };
        if ret == 0 && ws.ws_col > 40 {
            return ws.ws_col as usize;
        }
    }
    100
}

/// Format a duration in seconds to human-readable string.
pub fn format_duration(start: i64, end: Option<i64>) -> String {
    let Some(end) = end else { return "-".into() };
    let secs = (end - start).max(0);
    if secs < 1 {
        "<1s".into()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format a unix timestamp as relative time (e.g., "3h ago").
pub fn format_relative_time(ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let ago = (now - ts).max(0);
    if ago < 60 {
        "just now".into()
    } else if ago < 3600 {
        format!("{}m ago", ago / 60)
    } else if ago < 86400 {
        format!("{}h ago", ago / 3600)
    } else {
        format!("{}d ago", ago / 86400)
    }
}

/// Truncate a command string to fit terminal width.
pub fn truncate_command(cmd: &str, max_width: usize) -> String {
    let cmd = cmd.trim().replace('\n', " ");
    if cmd.len() <= max_width {
        cmd
    } else {
        format!("{}...", &cmd[..max_width.saturating_sub(3)])
    }
}

/// Format a source label with color.
pub fn source_label(source: &str) -> String {
    match source {
        "claude_code" => format!("{CYAN}agent{RESET}"),
        "human" => format!("{DIM}human{RESET}"),
        other => format!("{DIM}{other}{RESET}"),
    }
}

/// Parse a duration string like "2h", "30m", "7d" and return the unix timestamp
/// for that duration ago from now.
pub fn parse_duration_ago(input: &str) -> Result<i64, crate::error::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let input = input.trim().to_lowercase();
    let (num_str, unit) = if input.ends_with('h') {
        (&input[..input.len() - 1], 3600i64)
    } else if input.ends_with('m') {
        (&input[..input.len() - 1], 60i64)
    } else if input.ends_with('d') {
        (&input[..input.len() - 1], 86400i64)
    } else if input.ends_with('s') {
        (&input[..input.len() - 1], 1i64)
    } else {
        return Err(crate::error::Error::Db(
            format!("invalid duration: {input}. Use format like '30s', '2h', '30m', '7d'")
        ));
    };

    let num: i64 = num_str.parse().map_err(|_| {
        crate::error::Error::Db(format!("invalid duration number: {num_str}"))
    })?;

    Ok(now - (num * unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_sub_second() {
        assert_eq!(format_duration(100, Some(100)), "<1s");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(100, Some(145)), "45s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(0, Some(125)), "2m5s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(0, Some(3725)), "1h2m");
    }

    #[test]
    fn format_duration_no_end() {
        assert_eq!(format_duration(100, None), "-");
    }
}
