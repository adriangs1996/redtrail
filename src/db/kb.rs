use crate::error::Error;
use rusqlite::{Connection, params};

pub fn add_host(
    conn: &Connection,
    session_id: &str,
    ip: &str,
    os: Option<&str>,
    hostname: Option<&str>,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT OR IGNORE INTO hosts (session_id, ip, os, hostname) VALUES (?1, ?2, ?3, ?4)",
        params![session_id, ip, os, hostname],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM hosts WHERE session_id = ?1 AND ip = ?2",
            params![session_id, ip],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
}

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

pub fn add_port(
    conn: &Connection,
    session_id: &str,
    host_ip: &str,
    port: i64,
    protocol: Option<&str>,
    service: Option<&str>,
    version: Option<&str>,
) -> Result<i64, Error> {
    let host_id = add_host(conn, session_id, host_ip, None, None)?;
    conn.execute(
        "INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![session_id, host_id, port, protocol.unwrap_or("tcp"), service, version],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM ports WHERE host_id = ?1 AND port = ?2 AND protocol = ?3",
            params![host_id, port, protocol.unwrap_or("tcp")],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
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

pub fn add_credential(
    conn: &Connection,
    session_id: &str,
    username: &str,
    password: Option<&str>,
    hash: Option<&str>,
    service: Option<&str>,
    host: Option<&str>,
    source: Option<&str>,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO credentials (session_id, username, password, hash, service, host, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![session_id, username, password, hash, service, host, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn add_flag(
    conn: &Connection,
    session_id: &str,
    value: &str,
    source: Option<&str>,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO flags (session_id, value, source) VALUES (?1, ?2, ?3)",
        params![session_id, value, source],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn add_access(
    conn: &Connection,
    session_id: &str,
    host: &str,
    user: &str,
    level: &str,
    method: Option<&str>,
) -> Result<i64, Error> {
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
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn add_web_path(
    conn: &Connection,
    session_id: &str,
    host_ip: &str,
    port: i64,
    scheme: &str,
    path: &str,
    status_code: Option<i64>,
    content_length: Option<i64>,
    content_type: Option<&str>,
    redirect_to: Option<&str>,
    source: Option<&str>,
) -> Result<i64, Error> {
    let host_id = add_host(conn, session_id, host_ip, None, None)?;
    conn.execute(
        "INSERT OR IGNORE INTO web_paths (session_id, host_id, port, scheme, path, status_code, content_length, content_type, redirect_to, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![session_id, host_id, port, scheme, path, status_code, content_length, content_type, redirect_to, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM web_paths WHERE session_id = ?1 AND host_id = ?2 AND port = ?3 AND path = ?4",
            params![session_id, host_id, port, path],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
}

pub fn add_vuln(
    conn: &Connection,
    session_id: &str,
    host_ip: &str,
    port: i64,
    name: &str,
    severity: Option<&str>,
    cve: Option<&str>,
    url: Option<&str>,
    detail: Option<&str>,
    source: Option<&str>,
) -> Result<i64, Error> {
    let host_id = add_host(conn, session_id, host_ip, None, None)?;
    conn.execute(
        "INSERT OR IGNORE INTO vulns (session_id, host_id, port, name, severity, cve, url, detail, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![session_id, host_id, port, name, severity, cve, url, detail, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM vulns WHERE session_id = ?1 AND host_id = ?2 AND port = ?3 AND name = ?4",
            params![session_id, host_id, port, name],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
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
    fn test_add_host_then_list_returns_it() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general")
            .unwrap();
        db.add_host("s1", "10.10.10.1", Some("Linux"), Some("box1"))
            .unwrap();
        let hosts = db.list_hosts("s1").unwrap();

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0]["ip"], "10.10.10.1");
        assert_eq!(hosts[0]["os"], "Linux");
        assert_eq!(hosts[0]["hostname"], "box1");
        assert_eq!(hosts[0]["status"], "up");
    }

    #[test]
    fn test_add_web_path_and_list() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_host("s1", "10.10.10.1", None, None).unwrap();
        let id = db.add_web_path(
            "s1", "10.10.10.1", 80, "http", "/admin",
            Some(200), Some(1234), Some("text/html"), None, Some("gobuster"),
        ).unwrap();
        assert!(id > 0);
        let paths = db.list_web_paths("s1", None).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0]["path"], "/admin");
        assert_eq!(paths[0]["status_code"], 200);
        assert_eq!(paths[0]["source"], "gobuster");
    }

    #[test]
    fn test_add_web_path_auto_creates_host() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_web_path(
            "s1", "10.10.10.99", 443, "https", "/login",
            Some(200), None, None, None, None,
        ).unwrap();
        let hosts = db.list_hosts("s1").unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0]["ip"], "10.10.10.99");
    }

    #[test]
    fn test_add_web_path_dedup() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_web_path("s1", "10.10.10.1", 80, "http", "/admin", Some(200), None, None, None, None).unwrap();
        db.add_web_path("s1", "10.10.10.1", 80, "http", "/admin", Some(403), None, None, None, None).unwrap();
        let paths = db.list_web_paths("s1", None).unwrap();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_list_web_paths_host_filter() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_web_path("s1", "10.10.10.1", 80, "http", "/a", None, None, None, None, None).unwrap();
        db.add_web_path("s1", "10.10.10.2", 80, "http", "/b", None, None, None, None, None).unwrap();
        let paths = db.list_web_paths("s1", Some("10.10.10.1")).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0]["path"], "/a");
    }

    #[test]
    fn test_add_vuln_and_list() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        let id = db.add_vuln(
            "s1", "10.10.10.1", 80, "Apache Path Traversal",
            Some("high"), Some("CVE-2021-41773"),
            Some("http://10.10.10.1/cgi-bin/.."), Some("traversal"), Some("nuclei"),
        ).unwrap();
        assert!(id > 0);
        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["name"], "Apache Path Traversal");
        assert_eq!(vulns[0]["severity"], "high");
        assert_eq!(vulns[0]["cve"], "CVE-2021-41773");
    }

    #[test]
    fn test_add_vuln_host_level_no_port() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_vuln(
            "s1", "10.10.10.1", 0, "Outdated OS",
            Some("medium"), None, None, None, None,
        ).unwrap();
        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["port"], 0);
    }

    #[test]
    fn test_add_vuln_dedup() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_vuln("s1", "10.10.10.1", 80, "XSS", Some("medium"), None, None, None, None).unwrap();
        db.add_vuln("s1", "10.10.10.1", 80, "XSS", Some("high"), None, None, None, None).unwrap();
        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
    }

    #[test]
    fn test_list_vulns_severity_filter() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_vuln("s1", "10.10.10.1", 80, "XSS", Some("medium"), None, None, None, None).unwrap();
        db.add_vuln("s1", "10.10.10.1", 443, "SQLi", Some("critical"), None, None, None, None).unwrap();
        let vulns = db.list_vulns("s1", None, Some("critical")).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["name"], "SQLi");
    }

    #[test]
    fn test_add_port_auto_creates_host() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general")
            .unwrap();
        db.add_port(
            "s1",
            "10.10.10.5",
            22,
            Some("tcp"),
            Some("ssh"),
            Some("OpenSSH 8.9"),
        )
        .unwrap();

        let hosts = db.list_hosts("s1").unwrap();
        assert_eq!(hosts.len(), 1, "port add should auto-create host");
        assert_eq!(hosts[0]["ip"], "10.10.10.5");

        let ports = db.list_ports("s1", None).unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0]["port"], 22);
        assert_eq!(ports[0]["service"], "ssh");
        assert_eq!(ports[0]["version"], "OpenSSH 8.9");
    }
}
