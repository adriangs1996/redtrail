use crate::db::kb;
use crate::error::Error;
use rusqlite::Connection;
use std::collections::BTreeMap;

const MAX_BRIEFING_CHARS: usize = 8000;

pub struct BriefingLimits {
    pub max_hosts: usize,
    pub max_interesting_paths: usize,
    pub max_commands: usize,
    pub max_notes: usize,
    pub max_evidence_per_hyp: usize,
}

impl Default for BriefingLimits {
    fn default() -> Self {
        Self {
            max_hosts: 20,
            max_interesting_paths: 20,
            max_commands: 15,
            max_notes: 10,
            max_evidence_per_hyp: 3,
        }
    }
}

impl BriefingLimits {
    pub fn reduce(&self) -> Self {
        Self {
            max_hosts: self.max_hosts / 2,
            max_interesting_paths: self.max_interesting_paths / 2,
            max_commands: self.max_commands / 2,
            max_notes: self.max_notes / 2,
            max_evidence_per_hyp: self.max_evidence_per_hyp.saturating_sub(1),
        }
    }
}

pub fn build_briefing(conn: &Connection, session_id: &str) -> Result<String, Error> {
    let limits = BriefingLimits::default();
    let result = build_briefing_with_limits(conn, session_id, &limits)?;
    if result.len() <= MAX_BRIEFING_CHARS {
        return Ok(result);
    }
    let l1 = limits.reduce();
    let result = build_briefing_with_limits(conn, session_id, &l1)?;
    if result.len() <= MAX_BRIEFING_CHARS {
        return Ok(result);
    }
    let l2 = l1.reduce();
    build_briefing_with_limits(conn, session_id, &l2)
}

pub fn build_briefing_with_limits(
    conn: &Connection,
    session_id: &str,
    limits: &BriefingLimits,
) -> Result<String, Error> {
    let mut out = String::new();

    build_hosts_section(conn, session_id, limits, &mut out)?;
    build_web_paths_section(conn, session_id, limits, &mut out)?;

    if out.is_empty() {
        return Ok("## KB Status\nNo data recorded yet. The knowledge base is empty.\n".into());
    }
    Ok(out)
}

fn build_hosts_section(
    conn: &Connection,
    session_id: &str,
    limits: &BriefingLimits,
    out: &mut String,
) -> Result<(), Error> {
    let hosts = kb::list_hosts(conn, session_id)?;
    if hosts.is_empty() {
        return Ok(());
    }

    out.push_str("## Hosts\n");
    let total = hosts.len();
    let shown = total.min(limits.max_hosts);

    for host in &hosts[..shown] {
        let ip = host["ip"].as_str().unwrap_or("?");
        let hostname = host["hostname"].as_str().unwrap_or("");
        let os = host["os"].as_str().unwrap_or("");
        let status = host["status"].as_str().unwrap_or("up");

        let host_part = if hostname.is_empty() {
            ip.to_string()
        } else {
            format!("{ip} ({hostname})")
        };
        let os_part = if os.is_empty() {
            String::new()
        } else {
            format!(" [{os}]")
        };
        out.push_str(&format!(
            "{host_part}{os_part} {}\n",
            status.to_uppercase()
        ));

        let ports = kb::list_ports(conn, session_id, Some(ip))?;
        for p in &ports {
            let port_num = p["port"].as_i64().unwrap_or(0);
            let proto = p["protocol"].as_str().unwrap_or("tcp");
            let service = p["service"].as_str().unwrap_or("");
            let version = p["version"].as_str().unwrap_or("");
            let mut line = format!("  {port_num}/{proto}");
            if !service.is_empty() {
                line.push_str(&format!(" {service}"));
            }
            if !version.is_empty() {
                line.push_str(&format!(" {version}"));
            }
            line.push('\n');
            out.push_str(&line);
        }
    }

    if total > shown {
        out.push_str(&format!("... and {} more hosts\n", total - shown));
    }

    Ok(())
}

const INTERESTING_PATTERNS: &[&str] = &[
    "admin", "backup", "config", ".git", ".env", ".htaccess",
    "wp-admin", "phpmyadmin", ".sql", ".bak", ".old",
];
const INTERESTING_CONTENT_TYPES: &[&str] = &["zip", "sql", "json", "xml", "pdf", "tar", "gz", "bak"];
const BORING_STATUS_CODES: &[i64] = &[200, 301, 302, 304];

fn is_interesting_path(wp: &serde_json::Value) -> bool {
    let code = wp["status_code"].as_i64().unwrap_or(200);
    if !BORING_STATUS_CODES.contains(&code) { return true; }
    let path = wp["path"].as_str().unwrap_or("");
    if INTERESTING_PATTERNS.iter().any(|p| path.contains(p)) { return true; }
    let ct = wp["content_type"].as_str().unwrap_or("");
    INTERESTING_CONTENT_TYPES.iter().any(|t| ct.contains(t))
}

fn human_size(bytes: i64) -> String {
    if bytes >= 1_048_576 { format!("{:.1}MB", bytes as f64 / 1_048_576.0) }
    else if bytes >= 1024 { format!("{:.1}KB", bytes as f64 / 1024.0) }
    else { format!("{bytes}B") }
}

fn build_web_paths_section(
    conn: &Connection,
    session_id: &str,
    limits: &BriefingLimits,
    out: &mut String,
) -> Result<(), Error> {
    let paths = kb::list_web_paths(conn, session_id, None)?;
    if paths.is_empty() {
        return Ok(());
    }

    let mut grouped: BTreeMap<String, Vec<&serde_json::Value>> = BTreeMap::new();
    for wp in &paths {
        let ip = wp["ip"].as_str().unwrap_or("?");
        let port = wp["port"].as_i64().unwrap_or(80);
        let key = format!("{ip}:{port}");
        grouped.entry(key).or_default().push(wp);
    }

    out.push_str("## Web Paths\n");
    for (endpoint, wps) in &grouped {
        let total = wps.len();
        let mut code_counts: BTreeMap<i64, usize> = BTreeMap::new();
        for wp in wps {
            let code = wp["status_code"].as_i64().unwrap_or(200);
            *code_counts.entry(code).or_default() += 1;
        }
        let summary: Vec<String> = code_counts.iter().map(|(c, n)| format!("{n}\u{00d7}{c}")).collect();
        out.push_str(&format!("{endpoint} \u{2014} {total} paths ({})\n", summary.join(", ")));

        let interesting: Vec<&serde_json::Value> = wps.iter().filter(|w| is_interesting_path(w)).copied().collect();
        let shown = interesting.len().min(limits.max_interesting_paths);
        for wp in &interesting[..shown] {
            let path = wp["path"].as_str().unwrap_or("?");
            let code = wp["status_code"].as_i64().unwrap_or(200);
            let ct = wp["content_type"].as_str().unwrap_or("");
            let cl = wp["content_length"].as_i64().unwrap_or(0);

            let mut detail = String::new();
            if !BORING_STATUS_CODES.contains(&code) {
                detail = format!("{code}");
            }
            if !ct.is_empty() && ct != "text/html" {
                if !detail.is_empty() { detail.push_str(", "); }
                detail.push_str(ct);
            }
            if cl > 0 {
                if !detail.is_empty() { detail.push_str(", "); }
                detail.push_str(&human_size(cl));
            }

            if detail.is_empty() {
                out.push_str(&format!("  {path} \u{2192} {code}\n"));
            } else {
                out.push_str(&format!("  {path} \u{2192} {detail}\n"));
            }
        }

        let remaining = total - shown;
        if remaining > 0 {
            out.push_str(&format!("  ... +{remaining} more paths\n"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target) VALUES ('s1', 'test', '/tmp', '10.10.10.1')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn briefing_empty_kb() {
        let conn = setup();
        let result = build_briefing(&conn, "s1").unwrap();
        assert!(result.contains("No data recorded yet"));
    }

    #[test]
    fn briefing_hosts_with_ports() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip, hostname, os, status) VALUES ('s1', '10.10.10.1', 'target.htb', 'Linux', 'up')",
            [],
        ).unwrap();
        let host_id: i64 = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO ports (session_id, host_id, port, protocol, service, version) VALUES ('s1', ?1, 22, 'tcp', 'ssh', 'OpenSSH 8.9')",
            [host_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO ports (session_id, host_id, port, protocol, service, version) VALUES ('s1', ?1, 80, 'tcp', 'http', 'Apache 2.4')",
            [host_id],
        ).unwrap();

        let result = build_briefing(&conn, "s1").unwrap();
        assert!(result.contains("## Hosts"));
        assert!(result.contains("10.10.10.1"));
        assert!(result.contains("target.htb"));
        assert!(result.contains("22/tcp ssh OpenSSH 8.9"));
        assert!(result.contains("80/tcp http Apache 2.4"));
    }

    fn insert_host(conn: &Connection, ip: &str) -> i64 {
        conn.execute(
            "INSERT OR IGNORE INTO hosts (session_id, ip, status) VALUES ('s1', ?1, 'up')",
            [ip],
        ).unwrap();
        conn.query_row(
            "SELECT id FROM hosts WHERE session_id = 's1' AND ip = ?1",
            [ip],
            |r| r.get(0),
        ).unwrap()
    }

    fn insert_web_path(conn: &Connection, host_id: i64, port: i64, path: &str, status_code: i64, content_type: &str, content_length: i64) {
        conn.execute(
            "INSERT INTO web_paths (session_id, host_id, port, scheme, path, status_code, content_type, content_length) VALUES ('s1', ?1, ?2, 'http', ?3, ?4, ?5, ?6)",
            rusqlite::params![host_id, port, path, status_code, content_type, content_length],
        ).unwrap();
    }

    #[test]
    fn briefing_web_paths_grouping() {
        let conn = setup();
        let hid = insert_host(&conn, "10.10.10.1");
        for i in 0..10 {
            insert_web_path(&conn, hid, 80, &format!("/page{i}"), 200, "text/html", 1024);
        }
        insert_web_path(&conn, hid, 80, "/admin", 403, "text/html", 0);
        insert_web_path(&conn, hid, 80, "/api/debug", 500, "text/html", 0);

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Web Paths"), "missing header");
        assert!(result.contains("/admin"), "missing /admin");
        assert!(result.contains("403"), "missing 403 status");
        assert!(result.contains("/api/debug"), "missing /api/debug");
        assert!(result.contains("500"), "missing 500 status");
        assert!(result.contains("10.10.10.1:80"), "missing endpoint");
    }

    #[test]
    fn briefing_web_paths_interesting_patterns() {
        let conn = setup();
        let hid = insert_host(&conn, "10.10.10.1");
        insert_web_path(&conn, hid, 80, "/.git/config", 200, "text/plain", 100);
        insert_web_path(&conn, hid, 80, "/.env", 200, "text/plain", 50);

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("/.git/config"), "pattern-matched .git path missing");
        assert!(result.contains("/.env"), "pattern-matched .env path missing");
    }

    #[test]
    fn briefing_web_paths_overflow_hint() {
        let conn = setup();
        let hid = insert_host(&conn, "10.10.10.1");
        for i in 0..60 {
            insert_web_path(&conn, hid, 80, &format!("/boring{i}"), 200, "text/html", 1024);
        }

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("... +"), "missing overflow hint");
    }

    #[test]
    fn briefing_hosts_truncation() {
        let conn = setup();
        for i in 0..25 {
            conn.execute(
                "INSERT INTO hosts (session_id, ip, status) VALUES ('s1', ?1, 'up')",
                [format!("10.10.10.{i}")],
            ).unwrap();
        }

        let limits = BriefingLimits {
            max_hosts: 20,
            ..BriefingLimits::default()
        };
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("... and 5 more hosts"));
    }
}
