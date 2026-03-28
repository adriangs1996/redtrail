use crate::error::Error;
use rusqlite::Connection;

pub fn run(conn: &Connection, db_path: Option<&str>) -> Result<(), Error> {
    let cmd_count: i64 = conn
        .query_row("SELECT count(*) FROM commands", [], |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))?;

    let session_count: i64 = conn
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))?;

    let failed_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM commands WHERE exit_code IS NOT NULL AND exit_code != 0",
            [],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let db_size = if let Some(path) = db_path {
        match std::fs::metadata(path) {
            Ok(m) => {
                let bytes = m.len();
                if bytes > 1_048_576 {
                    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
                } else {
                    format!("{:.1} KB", bytes as f64 / 1024.0)
                }
            }
            Err(_) => "unknown".to_string(),
        }
    } else {
        "in-memory".to_string()
    };

    let agent_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM commands WHERE tool_name IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let human_count = cmd_count - agent_count;

    println!("Database:   {}", db_path.unwrap_or("in-memory"));
    println!("DB size:    {db_size}");
    println!("Commands:   {cmd_count}");
    println!("  Human:    {human_count}");
    println!("  Agent:    {agent_count}");
    println!("Sessions:   {session_count}");
    println!("Failures:   {failed_count}");

    Ok(())
}
