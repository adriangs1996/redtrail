use crate::error::Error;
use rusqlite::Connection;

pub fn get_global_config(conn: &Connection) -> Result<Vec<(String, String)>, Error> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM global_config")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))
}

pub fn set_global_config(conn: &Connection, key: &str, value: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT OR REPLACE INTO global_config (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn get_session_config(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<(String, String)>, Error> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM session_config WHERE session_id = ?1")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![session_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))
}

pub fn set_session_config(
    conn: &Connection,
    session_id: &str,
    key: &str,
    value: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT OR REPLACE INTO session_config (session_id, key, value) VALUES (?1, ?2, ?3)",
        rusqlite::params![session_id, key, value],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    #[test]
    fn test_global_config_roundtrip() {
        let conn = test_conn();
        set_global_config(&conn, "general.autonomy", "cautious").unwrap();
        let rows = get_global_config(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], ("general.autonomy".into(), "cautious".into()));
    }

    #[test]
    fn test_global_config_replace() {
        let conn = test_conn();
        set_global_config(&conn, "general.autonomy", "cautious").unwrap();
        set_global_config(&conn, "general.autonomy", "autonomous").unwrap();
        let rows = get_global_config(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "autonomous");
    }

    #[test]
    fn test_session_config_roundtrip() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES ('s1', 'test', '/tmp', '', '', 'general')",
            [],
        ).unwrap();
        set_session_config(&conn, "s1", "general.autonomy", "cautious").unwrap();
        let rows = get_session_config(&conn, "s1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], ("general.autonomy".into(), "cautious".into()));
    }

    #[test]
    fn test_session_config_isolated() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, active, target, scope, goal) VALUES ('s1', 'a', '/tmp/a', 1, '', '', 'general')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, active, target, scope, goal) VALUES ('s2', 'b', '/tmp/b', 1, '', '', 'general')",
            [],
        ).unwrap();
        set_session_config(&conn, "s1", "noise.threshold", "3").unwrap();
        set_session_config(&conn, "s2", "noise.threshold", "7").unwrap();
        let s1 = get_session_config(&conn, "s1").unwrap();
        let s2 = get_session_config(&conn, "s2").unwrap();
        assert_eq!(s1[0].1, "3");
        assert_eq!(s2[0].1, "7");
    }

    #[test]
    fn test_empty_global_config() {
        let conn = test_conn();
        let rows = get_global_config(&conn).unwrap();
        assert!(rows.is_empty());
    }
}
