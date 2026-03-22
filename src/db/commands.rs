use crate::error::Error;
use rusqlite::{Connection, params};

pub fn insert(
    conn: &Connection,
    session_id: &str,
    command: &str,
    tool: Option<&str>,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO command_history (session_id, command, tool) VALUES (?1, ?2, ?3)",
        params![session_id, command, tool],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn finish(
    conn: &Connection,
    id: i64,
    exit_code: i32,
    duration_ms: i64,
    output: &str,
) -> Result<(), Error> {
    let preview = if output.len() > 500 {
        &output[..500]
    } else {
        output
    };
    conn.execute(
        "UPDATE command_history SET exit_code = ?1, duration_ms = ?2, output = ?3, output_preview = ?4 WHERE id = ?5",
        params![exit_code, duration_ms, output, preview, id],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn get_for_extraction(
    conn: &Connection,
    id: i64,
) -> Result<(String, String, Option<String>, Option<String>), Error> {
    conn.query_row(
        "SELECT session_id, command, tool, output FROM command_history WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .map_err(|e| Error::Db(e.to_string()))
}

pub fn update_extraction_status(conn: &Connection, id: i64, status: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE command_history SET extraction_status = ?1 WHERE id = ?2",
        params![status, id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        db::session::create_session(&conn, "s1", "test", "/tmp/test", None, None, "general").unwrap();
        conn
    }

    #[test]
    fn test_command_capture_lifecycle() {
        let conn = setup_conn();

        let cmd_id = insert(&conn, "s1", "nmap -sV 10.10.10.1", Some("nmap")).unwrap();
        assert!(cmd_id > 0);

        finish(&conn, cmd_id, 0, 1500, "22/tcp open ssh OpenSSH 8.9").unwrap();

        let (session_id, command, tool, output) = get_for_extraction(&conn, cmd_id).unwrap();
        assert_eq!(session_id, "s1");
        assert_eq!(command, "nmap -sV 10.10.10.1");
        assert_eq!(tool.as_deref(), Some("nmap"));
        assert_eq!(output.as_deref(), Some("22/tcp open ssh OpenSSH 8.9"));

        update_extraction_status(&conn, cmd_id, "done").unwrap();
    }
}
