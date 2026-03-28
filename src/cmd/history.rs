use crate::core::db::{self, CommandFilter, CommandRow};
use crate::error::Error;
use rusqlite::Connection;

pub struct HistoryArgs<'a> {
    pub failed: bool,
    pub cmd: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub today: bool,
    pub search: Option<&'a str>,
    pub source: Option<&'a str>,
    pub tool: Option<&'a str>,
    pub verbose: bool,
    pub json: bool,
}

fn start_of_today() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    now - (now % 86400)
}

pub fn run(conn: &Connection, args: &HistoryArgs) -> Result<(), Error> {
    let commands = if let Some(query) = args.search {
        db::search_commands(conn, query, 50)?
    } else {
        let since = if args.today { Some(start_of_today()) } else { None };
        let filter = CommandFilter {
            failed_only: args.failed,
            command_binary: args.cmd,
            cwd: args.cwd,
            since,
            source: args.source,
            tool_name: args.tool,
            ..Default::default()
        };
        db::get_commands(conn, &filter)?
    };

    let use_json = args.json || (!args.verbose && !std::io::IsTerminal::is_terminal(&std::io::stdout()));
    if use_json {
        print_json(&commands)?;
    } else {
        print_table(&commands, args.verbose);
    }

    Ok(())
}

fn print_json(commands: &[CommandRow]) -> Result<(), Error> {
    let entries: Vec<serde_json::Value> = commands
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "command": c.command_raw,
                "binary": c.command_binary,
                "cwd": c.cwd,
                "exit_code": c.exit_code,
                "source": c.source,
                "timestamp_start": c.timestamp_start,
                "timestamp_end": c.timestamp_end,
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&entries).map_err(|e| Error::Db(e.to_string()))?
    );
    Ok(())
}

fn format_duration(start: i64, end: Option<i64>) -> String {
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

fn format_relative_time(ts: i64) -> String {
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

// ANSI color helpers
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn truncate_command(cmd: &str, max_width: usize) -> String {
    // Strip leading/trailing whitespace, collapse to single line
    let cmd = cmd.trim().replace('\n', " ");
    if cmd.len() <= max_width {
        cmd
    } else {
        format!("{}...", &cmd[..max_width.saturating_sub(3)])
    }
}

fn source_label(source: &str) -> String {
    match source {
        "claude_code" => format!("{CYAN}agent{RESET}"),
        "human" => format!("{DIM}human{RESET}"),
        other => format!("{DIM}{other}{RESET}"),
    }
}

fn print_table(commands: &[CommandRow], verbose: bool) {
    if commands.is_empty() {
        println!("{DIM}No commands found.{RESET}");
        return;
    }

    // Determine terminal width for command column, fallback to 100
    let term_width = terminal_width().unwrap_or(100);
    // Fixed columns: "| EXIT | DURATION |   WHEN   | SOURCE |  COMMAND  |"
    // Widths:          6      10         10         8        rest
    // Borders + padding: 6 pipes + spaces ≈ 18
    let fixed_overhead = 6 + 10 + 10 + 8 + 18;
    let cmd_width = term_width.saturating_sub(fixed_overhead).max(20);

    // Precompute rows to measure actual max widths
    struct Row {
        exit_str: String,
        exit_plain: String,
        duration: String,
        time: String,
        source_str: String,
        source_plain: String,
        command: String,
    }

    let rows: Vec<Row> = commands
        .iter()
        .map(|c| {
            let (exit_str, exit_plain) = match c.exit_code {
                Some(0) => (format!("{GREEN}0{RESET}"), "0".to_string()),
                Some(code) => (format!("{RED}{code}{RESET}"), format!("{code}")),
                None => ("-".into(), "-".into()),
            };
            let duration = format_duration(c.timestamp_start, c.timestamp_end);
            let time = format_relative_time(c.timestamp_start);
            let source_str = source_label(&c.source);
            let source_plain = if c.source == "claude_code" { "agent" } else { &c.source };
            let command = truncate_command(&c.command_raw, cmd_width);
            Row {
                exit_str,
                exit_plain: exit_plain.to_string(),
                duration,
                time,
                source_str,
                source_plain: source_plain.to_string(),
                command,
            }
        })
        .collect();

    // Calculate column widths (min widths from headers)
    let w_exit = rows.iter().map(|r| r.exit_plain.len()).max().unwrap().max(4);
    let w_dur = rows.iter().map(|r| r.duration.len()).max().unwrap().max(8);
    let w_time = rows.iter().map(|r| r.time.len()).max().unwrap().max(4);
    let w_src = rows.iter().map(|r| r.source_plain.len()).max().unwrap().max(6);
    let w_cmd = rows.iter().map(|r| r.command.len()).max().unwrap().max(7);

    // Border line
    let border = format!(
        "{DIM}+-{}-+-{}-+-{}-+-{}-+-{}-+{RESET}",
        "-".repeat(w_exit),
        "-".repeat(w_dur),
        "-".repeat(w_time),
        "-".repeat(w_src),
        "-".repeat(w_cmd),
    );

    // Header
    println!("{border}");
    println!(
        "{DIM}|{RESET} {BOLD}{:>w_exit$}{RESET} {DIM}|{RESET} {BOLD}{:<w_dur$}{RESET} {DIM}|{RESET} {BOLD}{:>w_time$}{RESET} {DIM}|{RESET} {BOLD}{:<w_src$}{RESET} {DIM}|{RESET} {BOLD}{:<w_cmd$}{RESET} {DIM}|{RESET}",
        "EXIT", "DURATION", "WHEN", "SOURCE", "COMMAND",
    );
    println!("{border}");

    // Rows
    for (i, (c, row)) in commands.iter().zip(rows.iter()).enumerate() {
        // Pad the colored strings to match the plain-text width
        let exit_pad = w_exit.saturating_sub(row.exit_plain.len());
        let src_pad = w_src.saturating_sub(row.source_plain.len());

        println!(
            "{DIM}|{RESET} {}{} {DIM}|{RESET} {:<w_dur$} {DIM}|{RESET} {YELLOW}{:>w_time$}{RESET} {DIM}|{RESET} {}{} {DIM}|{RESET} {:<w_cmd$} {DIM}|{RESET}",
            " ".repeat(exit_pad),
            row.exit_str,
            row.duration,
            row.time,
            row.source_str,
            " ".repeat(src_pad),
            row.command,
        );

        if verbose {
            if let Some(stdout) = &c.stdout {
                if !stdout.is_empty() {
                    let prefix = format!(
                        "{DIM}|{RESET} {:>w_exit$} {DIM}|{RESET}",
                        ""
                    );
                    for line in stdout.lines().take(10) {
                        println!("{prefix} {DIM}{line}{RESET}");
                    }
                    let count = stdout.lines().count();
                    if count > 10 {
                        println!("{prefix} {DIM}... ({} more lines){RESET}", count - 10);
                    }
                }
            }
            if let Some(stderr) = &c.stderr {
                if !stderr.is_empty() {
                    let prefix = format!(
                        "{DIM}|{RESET} {:>w_exit$} {DIM}|{RESET}",
                        ""
                    );
                    for line in stderr.lines().take(5) {
                        println!("{prefix} {RED}{DIM}{line}{RESET}");
                    }
                    let count = stderr.lines().count();
                    if count > 5 {
                        println!("{prefix} {RED}{DIM}... ({} more lines){RESET}", count - 5);
                    }
                }
            }
        }

        // Separate rows with a lighter divider (skip after last)
        if i < commands.len() - 1 && verbose {
            println!("{border}");
        }
    }

    // Bottom border
    println!("{border}");

    // Summary line
    let total = commands.len();
    let failed = commands.iter().filter(|c| matches!(c.exit_code, Some(code) if code != 0)).count();
    if failed > 0 {
        println!("{DIM}{total} commands ({RED}{failed} failed{RESET}{DIM}){RESET}");
    } else {
        println!("{DIM}{total} commands{RESET}");
    }
}

fn terminal_width() -> Option<usize> {
    if let Ok(val) = std::env::var("COLUMNS") {
        if let Ok(w) = val.parse::<usize>() {
            if w > 40 {
                return Some(w);
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdout().as_raw_fd();
        // Use nix's libc re-export for winsize ioctl
        let mut ws: nix::libc::winsize = unsafe { std::mem::zeroed() };
        let ret = unsafe { nix::libc::ioctl(fd, nix::libc::TIOCGWINSZ, &mut ws) };
        if ret == 0 && ws.ws_col > 40 {
            return Some(ws.ws_col as usize);
        }
    }
    None
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
