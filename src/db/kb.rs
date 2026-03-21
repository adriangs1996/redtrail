use crate::error::Error;
use rusqlite::{Connection, params};

pub fn list_hosts(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn
        .prepare("SELECT ip, hostname, os, status FROM hosts WHERE session_id = ?1 ORDER BY ip")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "ip": r.get::<_, String>(0)?,
                "hostname": r.get::<_, Option<String>>(1)?,
                "os": r.get::<_, Option<String>>(2)?,
                "status": r.get::<_, String>(3)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

pub fn list_ports(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
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

pub fn list_credentials(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT username, password, hash, service, host, source FROM credentials WHERE session_id = ?1"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "username": r.get::<_, String>(0)?,
                "password": r.get::<_, Option<String>>(1)?,
                "hash": r.get::<_, Option<String>>(2)?,
                "service": r.get::<_, Option<String>>(3)?,
                "host": r.get::<_, Option<String>>(4)?,
                "source": r.get::<_, Option<String>>(5)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

pub fn list_flags(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn
        .prepare("SELECT value, source, captured_at FROM flags WHERE session_id = ?1")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "value": r.get::<_, String>(0)?,
                "source": r.get::<_, Option<String>>(1)?,
                "captured_at": r.get::<_, String>(2)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

pub fn list_access(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn
        .prepare("SELECT host, user, level, method FROM access_levels WHERE session_id = ?1")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "host": r.get::<_, String>(0)?,
                "user": r.get::<_, String>(1)?,
                "level": r.get::<_, String>(2)?,
                "method": r.get::<_, Option<String>>(3)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

pub fn list_notes(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn
        .prepare("SELECT text, created_at FROM notes WHERE session_id = ?1")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "text": r.get::<_, String>(0)?,
                "created_at": r.get::<_, String>(1)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

pub fn list_web_paths(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "port": r.get::<_, i64>(1)?,
            "scheme": r.get::<_, String>(2)?,
            "path": r.get::<_, String>(3)?,
            "status_code": r.get::<_, Option<i64>>(4)?,
            "content_length": r.get::<_, Option<i64>>(5)?,
            "content_type": r.get::<_, Option<String>>(6)?,
            "redirect_to": r.get::<_, Option<String>>(7)?,
            "source": r.get::<_, Option<String>>(8)?,
        }))
    };
    if let Some(ip) = host_filter {
        let mut stmt = conn.prepare(
            "SELECT h.ip, w.port, w.scheme, w.path, w.status_code, w.content_length, w.content_type, w.redirect_to, w.source FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 AND h.ip = ?2 ORDER BY w.path"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id, ip], map_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .map(|r| r.map_err(|e| Error::Db(e.to_string())))
            .collect()
    } else {
        let mut stmt = conn.prepare(
            "SELECT h.ip, w.port, w.scheme, w.path, w.status_code, w.content_length, w.content_type, w.redirect_to, w.source FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 ORDER BY h.ip, w.path"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id], map_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .map(|r| r.map_err(|e| Error::Db(e.to_string())))
            .collect()
    }
}

pub fn list_vulns(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
    severity_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "port": r.get::<_, i64>(1)?,
            "name": r.get::<_, String>(2)?,
            "severity": r.get::<_, Option<String>>(3)?,
            "cve": r.get::<_, Option<String>>(4)?,
            "url": r.get::<_, Option<String>>(5)?,
            "detail": r.get::<_, Option<String>>(6)?,
            "source": r.get::<_, Option<String>>(7)?,
        }))
    };
    match (host_filter, severity_filter) {
        (Some(ip), Some(sev)) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND h.ip = ?2 AND v.severity = ?3 ORDER BY v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, ip, sev], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
        (Some(ip), None) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND h.ip = ?2 ORDER BY v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, ip], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
        (None, Some(sev)) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND v.severity = ?2 ORDER BY v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, sev], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 ORDER BY h.ip, v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
    }
}

pub fn list_history(
    conn: &Connection,
    session_id: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, command, exit_code, duration_ms, tool, started_at FROM command_history WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(params![session_id, limit as i64], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "command": r.get::<_, String>(1)?,
                "exit_code": r.get::<_, Option<i64>>(2)?,
                "duration_ms": r.get::<_, Option<i64>>(3)?,
                "tool": r.get::<_, Option<String>>(4)?,
                "started_at": r.get::<_, String>(5)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

pub fn search(
    conn: &Connection,
    session_id: &str,
    query: &str,
) -> Result<Vec<serde_json::Value>, Error> {
    let pattern = format!("%{query}%");
    let mut results: Vec<serde_json::Value> = Vec::new();

    let searches = [
        (
            "host",
            "SELECT 'host' as kind, ip as value FROM hosts WHERE session_id = ?1 AND (ip LIKE ?2 OR hostname LIKE ?2)",
        ),
        (
            "credential",
            "SELECT 'credential' as kind, username as value FROM credentials WHERE session_id = ?1 AND username LIKE ?2",
        ),
        (
            "note",
            "SELECT 'note' as kind, text as value FROM notes WHERE session_id = ?1 AND text LIKE ?2",
        ),
        (
            "command",
            "SELECT 'command' as kind, command as value FROM command_history WHERE session_id = ?1 AND command LIKE ?2",
        ),
        (
            "web_path",
            "SELECT 'web_path' as kind, path as value FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 AND (w.path LIKE ?2 OR h.ip LIKE ?2)",
        ),
        (
            "vuln",
            "SELECT 'vuln' as kind, name as value FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND (v.name LIKE ?2 OR v.cve LIKE ?2 OR h.ip LIKE ?2)",
        ),
    ];

    for (_kind, sql) in &searches {
        let mut stmt = conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, &pattern], |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        }).map_err(|e| Error::Db(e.to_string()))?;
        for r in rows {
            results.push(r.map_err(|e| Error::Db(e.to_string()))?);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use crate::db::{KnowledgeBase, SessionOps, open_in_memory};

    #[test]
    fn test_list_hosts_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        let hosts = db.list_hosts("s1").unwrap();
        assert!(hosts.is_empty());
    }

    #[test]
    fn test_list_ports_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        let ports = db.list_ports("s1", None).unwrap();
        assert!(ports.is_empty());
    }
}
