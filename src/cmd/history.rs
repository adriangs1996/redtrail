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

fn print_table(commands: &[CommandRow], verbose: bool) {
    for c in commands {
        let exit_str = match c.exit_code {
            Some(0) => "\x1b[32m0\x1b[0m".to_string(),
            Some(code) => format!("\x1b[31m{code}\x1b[0m"),
            None => "-".into(),
        };
        let duration = format_duration(c.timestamp_start, c.timestamp_end);
        let time = format_relative_time(c.timestamp_start);
        println!(
            "  {exit_str}  {duration:>6}  {time:>8}  {}",
            c.command_raw
        );
        if verbose {
            if let Some(stdout) = &c.stdout {
                if !stdout.is_empty() {
                    for line in stdout.lines() {
                        println!("    \x1b[2m{line}\x1b[0m");
                    }
                }
            }
            if let Some(stderr) = &c.stderr {
                if !stderr.is_empty() {
                    for line in stderr.lines() {
                        println!("    \x1b[31;2m{line}\x1b[0m");
                    }
                }
            }
        }
    }
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
