use crate::core::db;
use crate::error::Error;
use rusqlite::Connection;

pub fn run(conn: &Connection, since: Option<i64>) -> Result<(), Error> {
    let commands = db::get_commands(
        conn,
        &db::CommandFilter {
            since,
            limit: Some(10_000),
            ..Default::default()
        },
    )?;

    let entries: Vec<serde_json::Value> = commands
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "session_id": c.session_id,
                "command": c.command_raw,
                "binary": c.command_binary,
                "cwd": c.cwd,
                "exit_code": c.exit_code,
                "source": c.source,
                "timestamp_start": c.timestamp_start,
                "timestamp_end": c.timestamp_end,
                "hostname": c.hostname,
                "shell": c.shell,
                "stdout": c.stdout,
                "stderr": c.stderr,
                "redacted": c.redacted,
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&entries).unwrap());
    Ok(())
}
