use crate::core::db::{self, CommandFilter, CommandRow};
use crate::error::Error;
use rusqlite::Connection;

pub struct HistoryArgs<'a> {
    pub failed: bool,
    pub cmd: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub json: bool,
}

pub fn run(conn: &Connection, args: &HistoryArgs) -> Result<(), Error> {
    let filter = CommandFilter {
        failed_only: args.failed,
        command_binary: args.cmd,
        cwd: args.cwd,
        ..Default::default()
    };

    let commands = db::get_commands(conn, &filter)?;

    if args.json {
        print_json(&commands)?;
    } else {
        print_table(&commands);
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

fn print_table(commands: &[CommandRow]) {
    for c in commands {
        let exit = c
            .exit_code
            .map(|e| e.to_string())
            .unwrap_or_else(|| "?".into());
        let cwd = c.cwd.as_deref().unwrap_or("-");
        println!("{}\t{}\t{}\t{}", exit, cwd, c.command_raw, c.source);
    }
}
