use crate::error::Error;
use rusqlite::Connection;

use super::types::{NewSession, SessionRow};

pub fn create_session(conn: &Connection, s: &NewSession) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO sessions (id, started_at, cwd_initial, hostname, shell, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, now, s.cwd_initial, s.hostname, s.shell, s.source],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(id)
}

pub fn get_session(conn: &Connection, id: &str) -> Result<SessionRow, Error> {
    conn.query_row(
        "SELECT id, started_at, ended_at, cwd_initial, hostname, shell, source, command_count, error_count
         FROM sessions WHERE id = ?1",
        [id],
        |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                cwd_initial: row.get(3)?,
                hostname: row.get(4)?,
                shell: row.get(5)?,
                source: row.get(6)?,
                command_count: row.get(7)?,
                error_count: row.get(8)?,
            })
        },
    )
    .map_err(|e| Error::Db(e.to_string()))
}

pub fn list_sessions(conn: &Connection, limit: usize) -> Result<Vec<SessionRow>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT id, started_at, ended_at, cwd_initial, hostname, shell, source, command_count, error_count
             FROM sessions ORDER BY started_at DESC LIMIT ?1",
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                cwd_initial: row.get(3)?,
                hostname: row.get(4)?,
                shell: row.get(5)?,
                source: row.get(6)?,
                command_count: row.get(7)?,
                error_count: row.get(8)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Find or create a RedTrail session for a given agent session ID.
pub fn find_or_create_agent_session(
    conn: &Connection,
    agent_session_id: &str,
    cwd: Option<&str>,
    source: &str,
) -> Result<String, Error> {
    // Try to find existing session by agent_session_id
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM sessions WHERE agent_session_id = ?1 AND source = ?2",
            rusqlite::params![agent_session_id, source],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new session
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO sessions (id, started_at, cwd_initial, source, agent_session_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, now, cwd, source, agent_session_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(id)
}
