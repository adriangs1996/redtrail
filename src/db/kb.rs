use crate::error::Error;
use rusqlite::Connection;

/// Execute a parameterized query and collect results as JSON values.
///
/// Encapsulates the prepare → query_map → collect pattern that every KB query
/// uses, keeping individual query functions focused on their SQL and row mapping.
fn query_json(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
    map_row: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
    stmt.query_map(params, map_row)
        .map_err(|e| Error::Db(e.to_string()))?
        .map(|r| r.map_err(|e| Error::Db(e.to_string())))
        .collect()
}

/// Build a WHERE clause dynamically from optional filters, avoiding combinatorial
/// explosion of match arms for each filter combination.
struct FilteredQuery {
    sql: String,
    params: Vec<Box<dyn rusqlite::types::ToSql>>,
}

impl FilteredQuery {
    fn with_session_column(base_select: &str, session_col: &str, session_id: &str) -> Self {
        Self {
            sql: format!("{base_select} WHERE {session_col} = ?1"),
            params: vec![Box::new(session_id.to_string())],
        }
    }

    fn add_filter(&mut self, condition: &str, value: &str) {
        let idx = self.params.len() + 1;
        self.sql.push_str(&format!(" AND {condition} = ?{idx}"));
        self.params.push(Box::new(value.to_string()));
    }

    fn order_by(&mut self, clause: &str) {
        self.sql.push_str(&format!(" ORDER BY {clause}"));
    }

    fn limit(&mut self, n: usize) {
        let idx = self.params.len() + 1;
        self.sql.push_str(&format!(" LIMIT ?{idx}"));
        self.params.push(Box::new(n as i64));
    }

    fn param_refs(&self) -> Vec<&dyn rusqlite::types::ToSql> {
        self.params.iter().map(|p| p.as_ref()).collect()
    }
}

pub fn list_hosts(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    query_json(
        conn,
        "SELECT ip, hostname, os, status FROM hosts WHERE session_id = ?1 ORDER BY ip",
        &[&session_id as &dyn rusqlite::types::ToSql],
        |r| Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "hostname": r.get::<_, Option<String>>(1)?,
            "os": r.get::<_, Option<String>>(2)?,
            "status": r.get::<_, String>(3)?,
        })),
    )
}

pub fn list_ports(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let base = "SELECT h.ip, p.port, p.protocol, p.service, p.version \
                FROM ports p JOIN hosts h ON p.host_id = h.id";
    let mut q = FilteredQuery::with_session_column(base, "p.session_id", session_id);
    if let Some(ip) = host_filter {
        q.add_filter("h.ip", ip);
    }
    q.order_by("p.port");
    query_json(conn, &q.sql, &q.param_refs(), |r| Ok(serde_json::json!({
        "ip": r.get::<_, String>(0)?,
        "port": r.get::<_, i64>(1)?,
        "protocol": r.get::<_, String>(2)?,
        "service": r.get::<_, Option<String>>(3)?,
        "version": r.get::<_, Option<String>>(4)?,
    })))
}

pub fn list_credentials(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<serde_json::Value>, Error> {
    query_json(
        conn,
        "SELECT username, password, hash, service, host, source FROM credentials WHERE session_id = ?1",
        &[&session_id as &dyn rusqlite::types::ToSql],
        |r| Ok(serde_json::json!({
            "username": r.get::<_, String>(0)?,
            "password": r.get::<_, Option<String>>(1)?,
            "hash": r.get::<_, Option<String>>(2)?,
            "service": r.get::<_, Option<String>>(3)?,
            "host": r.get::<_, Option<String>>(4)?,
            "source": r.get::<_, Option<String>>(5)?,
        })),
    )
}

pub fn list_flags(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    query_json(
        conn,
        "SELECT value, source, captured_at FROM flags WHERE session_id = ?1",
        &[&session_id as &dyn rusqlite::types::ToSql],
        |r| Ok(serde_json::json!({
            "value": r.get::<_, String>(0)?,
            "source": r.get::<_, Option<String>>(1)?,
            "captured_at": r.get::<_, String>(2)?,
        })),
    )
}

pub fn list_access(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    query_json(
        conn,
        "SELECT host, user, level, method FROM access_levels WHERE session_id = ?1",
        &[&session_id as &dyn rusqlite::types::ToSql],
        |r| Ok(serde_json::json!({
            "host": r.get::<_, String>(0)?,
            "user": r.get::<_, String>(1)?,
            "level": r.get::<_, String>(2)?,
            "method": r.get::<_, Option<String>>(3)?,
        })),
    )
}

pub fn list_notes(conn: &Connection, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
    query_json(
        conn,
        "SELECT text, created_at FROM notes WHERE session_id = ?1",
        &[&session_id as &dyn rusqlite::types::ToSql],
        |r| Ok(serde_json::json!({
            "text": r.get::<_, String>(0)?,
            "created_at": r.get::<_, String>(1)?,
        })),
    )
}

pub fn list_web_paths(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let base = "SELECT h.ip, w.port, w.scheme, w.path, w.status_code, w.content_length, \
                w.content_type, w.redirect_to, w.source \
                FROM web_paths w JOIN hosts h ON w.host_id = h.id";
    let mut q = FilteredQuery::with_session_column(base, "w.session_id", session_id);
    if let Some(ip) = host_filter {
        q.add_filter("h.ip", ip);
        q.order_by("w.path");
    } else {
        q.order_by("h.ip, w.path");
    }
    query_json(conn, &q.sql, &q.param_refs(), |r| Ok(serde_json::json!({
        "ip": r.get::<_, String>(0)?,
        "port": r.get::<_, i64>(1)?,
        "scheme": r.get::<_, String>(2)?,
        "path": r.get::<_, String>(3)?,
        "status_code": r.get::<_, Option<i64>>(4)?,
        "content_length": r.get::<_, Option<i64>>(5)?,
        "content_type": r.get::<_, Option<String>>(6)?,
        "redirect_to": r.get::<_, Option<String>>(7)?,
        "source": r.get::<_, Option<String>>(8)?,
    })))
}

pub fn list_vulns(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
    severity_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let base = "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source \
                FROM vulns v JOIN hosts h ON v.host_id = h.id";
    let mut q = FilteredQuery::with_session_column(base, "v.session_id", session_id);
    if let Some(ip) = host_filter {
        q.add_filter("h.ip", ip);
    }
    if let Some(sev) = severity_filter {
        q.add_filter("v.severity", sev);
    }
    if host_filter.is_some() || severity_filter.is_some() {
        q.order_by("v.name");
    } else {
        q.order_by("h.ip, v.name");
    }
    query_json(conn, &q.sql, &q.param_refs(), |r| Ok(serde_json::json!({
        "ip": r.get::<_, String>(0)?,
        "port": r.get::<_, i64>(1)?,
        "name": r.get::<_, String>(2)?,
        "severity": r.get::<_, Option<String>>(3)?,
        "cve": r.get::<_, Option<String>>(4)?,
        "url": r.get::<_, Option<String>>(5)?,
        "detail": r.get::<_, Option<String>>(6)?,
        "source": r.get::<_, Option<String>>(7)?,
    })))
}

pub fn list_history(
    conn: &Connection,
    session_id: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut q = FilteredQuery::with_session_column(
        "SELECT id, command, exit_code, duration_ms, tool, started_at FROM command_history",
        "session_id",
        session_id,
    );
    q.order_by("id DESC");
    q.limit(limit);
    query_json(conn, &q.sql, &q.param_refs(), |r| Ok(serde_json::json!({
        "id": r.get::<_, i64>(0)?,
        "command": r.get::<_, String>(1)?,
        "exit_code": r.get::<_, Option<i64>>(2)?,
        "duration_ms": r.get::<_, Option<i64>>(3)?,
        "tool": r.get::<_, Option<String>>(4)?,
        "started_at": r.get::<_, String>(5)?,
    })))
}

pub fn search(
    conn: &Connection,
    session_id: &str,
    query: &str,
) -> Result<Vec<serde_json::Value>, Error> {
    let pattern = format!("%{query}%");
    let params: &[&dyn rusqlite::types::ToSql] = &[&session_id, &pattern];

    let searches = [
        "SELECT 'host' as kind, ip as value FROM hosts WHERE session_id = ?1 AND (ip LIKE ?2 OR hostname LIKE ?2)",
        "SELECT 'credential' as kind, username as value FROM credentials WHERE session_id = ?1 AND username LIKE ?2",
        "SELECT 'note' as kind, text as value FROM notes WHERE session_id = ?1 AND text LIKE ?2",
        "SELECT 'command' as kind, command as value FROM command_history WHERE session_id = ?1 AND command LIKE ?2",
        "SELECT 'web_path' as kind, path as value FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 AND (w.path LIKE ?2 OR h.ip LIKE ?2)",
        "SELECT 'vuln' as kind, name as value FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND (v.name LIKE ?2 OR v.cve LIKE ?2 OR h.ip LIKE ?2)",
    ];

    let mut results = Vec::new();
    for sql in &searches {
        let rows = query_json(conn, sql, params, |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        })?;
        results.extend(rows);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use crate::db::{KnowledgeBase, SessionOps, open_in_memory};

    #[test]
    fn test_list_hosts_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general").unwrap();
        let hosts = db.list_hosts("s1").unwrap();
        assert!(hosts.is_empty());
    }

    #[test]
    fn test_list_ports_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general").unwrap();
        let ports = db.list_ports("s1", None).unwrap();
        assert!(ports.is_empty());
    }
}
