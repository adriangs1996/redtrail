use rusqlite::{Connection, params};
use crate::error::Error;

pub fn add_host(conn: &Connection, session_id: &str, ip: &str, os: Option<&str>, hostname: Option<&str>) -> Result<i64, Error> {
    conn.execute(
        "INSERT OR IGNORE INTO hosts (session_id, ip, os, hostname) VALUES (?1, ?2, ?3, ?4)",
        params![session_id, ip, os, hostname],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn.query_row(
        "SELECT id FROM hosts WHERE session_id = ?1 AND ip = ?2",
        params![session_id, ip],
        |r| r.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
}

pub fn list_hosts(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT ip, hostname, os, status FROM hosts WHERE session_id = ?1 ORDER BY ip"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "hostname": r.get::<_, Option<String>>(1)?,
            "os": r.get::<_, Option<String>>(2)?,
            "status": r.get::<_, String>(3)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
}

pub fn add_port(conn: &Connection, session_id: &str, host_ip: &str, port: i64, protocol: Option<&str>, service: Option<&str>, version: Option<&str>) -> Result<i64, Error> {
    let host_id = add_host(conn, session_id, host_ip, None, None)?;
    conn.execute(
        "INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, host_id, port, protocol.unwrap_or("tcp"), service, version],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn.query_row(
        "SELECT id FROM ports WHERE host_id = ?1 AND port = ?2 AND protocol = ?3",
        params![host_id, port, protocol.unwrap_or("tcp")],
        |r| r.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
}

pub fn list_ports(conn: &Connection, session_id: &str, host_filter: Option<&str>) -> Result<Vec<serde_json::Value>, Error> {
    let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "port": r.get::<_, i64>(1)?,
            "protocol": r.get::<_, String>(2)?,
            "service": r.get::<_, Option<String>>(3)?,
            "version": r.get::<_, Option<String>>(4)?,
        }))
    };
    if let Some(ip) = host_filter {
        let mut stmt = conn.prepare(
            "SELECT h.ip, p.port, p.protocol, p.service, p.version FROM ports p JOIN hosts h ON p.host_id = h.id WHERE p.session_id = ?1 AND h.ip = ?2 ORDER BY p.port"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id, ip], map_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .map(|r| r.map_err(|e| Error::Db(e.to_string())))
            .collect()
    } else {
        let mut stmt = conn.prepare(
            "SELECT h.ip, p.port, p.protocol, p.service, p.version FROM ports p JOIN hosts h ON p.host_id = h.id WHERE p.session_id = ?1 ORDER BY p.port"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id], map_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .map(|r| r.map_err(|e| Error::Db(e.to_string())))
            .collect()
    }
}

pub fn add_credential(conn: &Connection, session_id: &str, username: &str, password: Option<&str>, hash: Option<&str>, service: Option<&str>, host: Option<&str>, source: Option<&str>) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO credentials (session_id, username, password, hash, service, host, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![session_id, username, password, hash, service, host, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn add_flag(conn: &Connection, session_id: &str, value: &str, source: Option<&str>) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO flags (session_id, value, source) VALUES (?1, ?2, ?3)",
        params![session_id, value, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn add_access(conn: &Connection, session_id: &str, host: &str, user: &str, level: &str, method: Option<&str>) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO access_levels (session_id, host, user, level, method) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_id, host, user, level, method],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn add_note(conn: &Connection, session_id: &str, text: &str) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO notes (session_id, text) VALUES (?1, ?2)",
        params![session_id, text],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn list_credentials(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT username, password, hash, service, host, source FROM credentials WHERE session_id = ?1"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(serde_json::json!({
            "username": r.get::<_, String>(0)?,
            "password": r.get::<_, Option<String>>(1)?,
            "hash": r.get::<_, Option<String>>(2)?,
            "service": r.get::<_, Option<String>>(3)?,
            "host": r.get::<_, Option<String>>(4)?,
            "source": r.get::<_, Option<String>>(5)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
}

pub fn list_flags(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT value, source, captured_at FROM flags WHERE session_id = ?1"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(serde_json::json!({
            "value": r.get::<_, String>(0)?,
            "source": r.get::<_, Option<String>>(1)?,
            "captured_at": r.get::<_, String>(2)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
}

pub fn list_access(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT host, user, level, method FROM access_levels WHERE session_id = ?1"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(serde_json::json!({
            "host": r.get::<_, String>(0)?,
            "user": r.get::<_, String>(1)?,
            "level": r.get::<_, String>(2)?,
            "method": r.get::<_, Option<String>>(3)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
}

pub fn list_notes(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT text, created_at FROM notes WHERE session_id = ?1"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok(serde_json::json!({
            "text": r.get::<_, String>(0)?,
            "created_at": r.get::<_, String>(1)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
}

pub fn list_history(conn: &Connection, session_id: &str, limit: usize) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, command, exit_code, duration_ms, tool, started_at FROM command_history WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![session_id, limit as i64], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, i64>(0)?,
            "command": r.get::<_, String>(1)?,
            "exit_code": r.get::<_, Option<i64>>(2)?,
            "duration_ms": r.get::<_, Option<i64>>(3)?,
            "tool": r.get::<_, Option<String>>(4)?,
            "started_at": r.get::<_, String>(5)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
}

pub fn search(conn: &Connection, session_id: &str, query: &str) -> Result<Vec<serde_json::Value>, Error> {
    let pattern = format!("%{query}%");
    let mut results: Vec<serde_json::Value> = Vec::new();

    let searches = [
        ("host", "SELECT 'host' as kind, ip as value FROM hosts WHERE session_id = ?1 AND (ip LIKE ?2 OR hostname LIKE ?2)"),
        ("credential", "SELECT 'credential' as kind, username as value FROM credentials WHERE session_id = ?1 AND username LIKE ?2"),
        ("note", "SELECT 'note' as kind, text as value FROM notes WHERE session_id = ?1 AND text LIKE ?2"),
        ("command", "SELECT 'command' as kind, command as value FROM command_history WHERE session_id = ?1 AND command LIKE ?2"),
    ];

    for (_kind, sql) in &searches {
        let mut stmt = conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, &pattern], |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        }).map_err(|e| Error::Db(e.to_string()))?;
        for r in rows { results.push(r.map_err(|e| Error::Db(e.to_string()))?); }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqliteDb;

    fn setup() -> SqliteDb {
        let db = SqliteDb::open_in_memory().unwrap();
        db.conn().execute(
            "INSERT INTO sessions (id, name) VALUES ('s1', 'test')", [],
        ).unwrap();
        db
    }

    #[test]
    fn test_add_host_then_list_returns_it() {
        let db = setup();
        add_host(db.conn(), "s1", "10.10.10.1", Some("Linux"), Some("box1")).unwrap();
        let hosts = list_hosts(db.conn(), "s1").unwrap();

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0]["ip"], "10.10.10.1");
        assert_eq!(hosts[0]["os"], "Linux");
        assert_eq!(hosts[0]["hostname"], "box1");
        assert_eq!(hosts[0]["status"], "up");
    }

    #[test]
    fn test_add_port_auto_creates_host() {
        let db = setup();
        add_port(db.conn(), "s1", "10.10.10.5", 22, Some("tcp"), Some("ssh"), Some("OpenSSH 8.9")).unwrap();

        let hosts = list_hosts(db.conn(), "s1").unwrap();
        assert_eq!(hosts.len(), 1, "port add should auto-create host");
        assert_eq!(hosts[0]["ip"], "10.10.10.5");

        let ports = list_ports(db.conn(), "s1", None).unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0]["port"], 22);
        assert_eq!(ports[0]["service"], "ssh");
        assert_eq!(ports[0]["version"], "OpenSSH 8.9");
    }
}
