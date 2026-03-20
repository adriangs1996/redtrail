use crate::error::Error;
use rusqlite::Connection;

pub fn save(conn: &Connection, session_id: &str, role: &str, content: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO chat_messages (session_id, role, content) VALUES (?1, ?2, ?3)",
        rusqlite::params![session_id, role, content],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn load(conn: &Connection, session_id: &str) -> Result<Vec<(String, String)>, Error> {
    let mut stmt = conn
        .prepare("SELECT role, content FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC")
        .map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))
}

pub fn clear(conn: &Connection, session_id: &str) -> Result<usize, Error> {
    conn.execute(
        "DELETE FROM chat_messages WHERE session_id = ?1",
        [session_id],
    )
    .map_err(|e| Error::Db(e.to_string()))
}
