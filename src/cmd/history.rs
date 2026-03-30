use crate::core::db::{self, CommandFilter, CommandRow};
use crate::core::fmt::ascii;
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

fn print_table(commands: &[CommandRow], verbose: bool) {
    if commands.is_empty() {
        println!("{}No commands found.{}", ascii::DIM, ascii::RESET);
        return;
    }

    // Determine terminal width for command column, fallback to 100
    let term_width = ascii::terminal_width();
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
                Some(0) => (format!("{}0{}", ascii::GREEN, ascii::RESET), "0".to_string()),
                Some(code) => (format!("{}{}{}", ascii::RED, code, ascii::RESET), format!("{code}")),
                None => ("-".into(), "-".into()),
            };
            let duration = ascii::format_duration(c.timestamp_start, c.timestamp_end);
            let time = ascii::format_relative_time(c.timestamp_start);
            let source_str = ascii::source_label(&c.source);
            let source_plain = if c.source == "claude_code" { "agent" } else { &c.source };
            let command = ascii::truncate_command(&c.command_raw, cmd_width);
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
        DIM = ascii::DIM,
        RESET = ascii::RESET,
    );

    // Header
    println!("{border}");
    println!(
        "{DIM}|{RESET} {BOLD}{:>w_exit$}{RESET} {DIM}|{RESET} {BOLD}{:<w_dur$}{RESET} {DIM}|{RESET} {BOLD}{:>w_time$}{RESET} {DIM}|{RESET} {BOLD}{:<w_src$}{RESET} {DIM}|{RESET} {BOLD}{:<w_cmd$}{RESET} {DIM}|{RESET}",
        "EXIT", "DURATION", "WHEN", "SOURCE", "COMMAND",
        DIM = ascii::DIM,
        RESET = ascii::RESET,
        BOLD = ascii::BOLD,
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
            DIM = ascii::DIM,
            RESET = ascii::RESET,
            YELLOW = ascii::YELLOW,
        );

        if verbose {
            if let Some(stdout) = &c.stdout {
                if !stdout.is_empty() {
                    let prefix = format!(
                        "{DIM}|{RESET} {:>w_exit$} {DIM}|{RESET}",
                        "",
                        DIM = ascii::DIM,
                        RESET = ascii::RESET,
                    );
                    for line in stdout.lines().take(10) {
                        println!("{prefix} {DIM}{line}{RESET}", DIM = ascii::DIM, RESET = ascii::RESET);
                    }
                    let count = stdout.lines().count();
                    if count > 10 {
                        println!("{prefix} {DIM}... ({} more lines){RESET}", count - 10, DIM = ascii::DIM, RESET = ascii::RESET);
                    }
                }
            }
            if let Some(stderr) = &c.stderr {
                if !stderr.is_empty() {
                    let prefix = format!(
                        "{DIM}|{RESET} {:>w_exit$} {DIM}|{RESET}",
                        "",
                        DIM = ascii::DIM,
                        RESET = ascii::RESET,
                    );
                    for line in stderr.lines().take(5) {
                        println!("{prefix} {RED}{DIM}{line}{RESET}", RED = ascii::RED, DIM = ascii::DIM, RESET = ascii::RESET);
                    }
                    let count = stderr.lines().count();
                    if count > 5 {
                        println!("{prefix} {RED}{DIM}... ({} more lines){RESET}", count - 5, RED = ascii::RED, DIM = ascii::DIM, RESET = ascii::RESET);
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
        println!(
            "{DIM}{total} commands ({RED}{failed} failed{RESET}{DIM}){RESET}",
            DIM = ascii::DIM,
            RED = ascii::RED,
            RESET = ascii::RESET,
        );
    } else {
        println!("{DIM}{total} commands{RESET}", DIM = ascii::DIM, RESET = ascii::RESET);
    }
}
