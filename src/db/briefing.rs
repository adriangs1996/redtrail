use crate::db::{hypothesis, kb};
use crate::error::Error;
use rusqlite::{Connection, params};
use std::collections::BTreeMap;
use std::sync::LazyLock;

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
    build_vulns_section(conn, session_id, &mut out)?;
    build_credentials_section(conn, session_id, &mut out)?;
    build_access_section(conn, session_id, &mut out)?;
    build_flags_section(conn, session_id, &mut out)?;
    build_hypotheses_section(conn, session_id, limits, &mut out)?;
    build_orphan_evidence_section(conn, session_id, &mut out)?;
    build_notes_section(conn, session_id, limits, &mut out)?;
    build_commands_section(conn, session_id, limits, &mut out)?;

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

fn build_vulns_section(
    conn: &Connection,
    session_id: &str,
    out: &mut String,
) -> Result<(), Error> {
    let vulns = kb::list_vulns(conn, session_id, None, None)?;
    if vulns.is_empty() {
        return Ok(());
    }

    let mut by_severity: BTreeMap<String, Vec<&serde_json::Value>> = BTreeMap::new();
    for v in &vulns {
        let sev = v["severity"].as_str().unwrap_or("info").to_string();
        by_severity.entry(sev).or_default().push(v);
    }

    let severity_order = ["critical", "high", "medium", "low", "info"];
    let total = vulns.len();
    let counts: Vec<String> = severity_order.iter()
        .filter_map(|s| by_severity.get(*s).map(|v| format!("{} {s}", v.len())))
        .collect();
    out.push_str(&format!("## Vulns ({total} total: {})\n", counts.join(", ")));

    for sev in &["critical", "high"] {
        if let Some(vs) = by_severity.get(*sev) {
            for v in vs {
                let ip = v["ip"].as_str().unwrap_or("?");
                let port = v["port"].as_i64().unwrap_or(0);
                let cve = v["cve"].as_str().unwrap_or("");
                let name = v["name"].as_str().unwrap_or("?");
                let cve_part = if cve.is_empty() { String::new() } else { format!("{cve} ") };
                out.push_str(&format!("{ip}:{port} {cve_part}[{sev}] {name}\n"));
            }
        }
    }

    let mut collapsed: Vec<&serde_json::Value> = Vec::new();
    for sev in &["medium", "low", "info"] {
        if let Some(vs) = by_severity.get(*sev) {
            collapsed.extend(vs);
        }
    }
    if !collapsed.is_empty() {
        let mut cat_counts: BTreeMap<String, usize> = BTreeMap::new();
        for v in &collapsed {
            let name = v["name"].as_str().unwrap_or("unknown");
            let cat = name.split(&['-', '_', ' '][..]).next().unwrap_or(name).to_string();
            *cat_counts.entry(cat).or_default() += 1;
        }
        let mut sorted_cats: Vec<(String, usize)> = cat_counts.into_iter().collect();
        sorted_cats.sort_by(|a, b| b.1.cmp(&a.1));
        let cat_summary: Vec<String> = sorted_cats.iter().map(|(c, n)| format!("{n}\u{00d7}{c}")).collect();
        out.push_str(&format!("... +{} medium/low/info ({})\n", collapsed.len(), cat_summary.join(", ")));
    }

    Ok(())
}

fn build_credentials_section(
    conn: &Connection,
    session_id: &str,
    out: &mut String,
) -> Result<(), Error> {
    let creds = kb::list_credentials(conn, session_id)?;
    if creds.is_empty() {
        return Ok(());
    }
    out.push_str("## Credentials\n");
    for c in &creds {
        let user = c["username"].as_str().unwrap_or("?");
        let pass = c["password"].as_str().unwrap_or("");
        let host = c["host"].as_str().unwrap_or("");
        let source = c["source"].as_str().unwrap_or("");
        let cred = if pass.is_empty() { user.to_string() } else { format!("{user}:{pass}") };
        let host_part = if host.is_empty() { String::new() } else { format!(" @ {host}") };
        let source_part = if source.is_empty() { String::new() } else { format!(" ({source})") };
        out.push_str(&format!("{cred}{host_part}{source_part}\n"));
    }
    Ok(())
}

fn build_access_section(
    conn: &Connection,
    session_id: &str,
    out: &mut String,
) -> Result<(), Error> {
    let access = kb::list_access(conn, session_id)?;
    if access.is_empty() {
        return Ok(());
    }
    out.push_str("## Access\n");
    for a in &access {
        let user = a["user"].as_str().unwrap_or("?");
        let host = a["host"].as_str().unwrap_or("?");
        let level = a["level"].as_str().unwrap_or("?");
        let method = a["method"].as_str().unwrap_or("");
        let method_part = if method.is_empty() { String::new() } else { format!(" method={method}") };
        out.push_str(&format!("{user}@{host} level={level}{method_part}\n"));
    }
    Ok(())
}

fn build_flags_section(
    conn: &Connection,
    session_id: &str,
    out: &mut String,
) -> Result<(), Error> {
    let flags = kb::list_flags(conn, session_id)?;
    if flags.is_empty() {
        return Ok(());
    }
    out.push_str("## Flags\n");
    for f in &flags {
        let value = f["value"].as_str().unwrap_or("?");
        let source = f["source"].as_str().unwrap_or("");
        let source_part = if source.is_empty() { String::new() } else { format!(" ({source})") };
        out.push_str(&format!("{value}{source_part}\n"));
    }
    Ok(())
}

static RE_IP: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap()
});
static RE_WORDLIST: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(-[wP]|--wordlist)\s+\S+").unwrap()
});
static RE_OUTPUT: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(-o|--output)\s+\S+").unwrap()
});

fn normalize_command(cmd: &str) -> String {
    let s = RE_WORDLIST.replace_all(cmd, "");
    let s = RE_OUTPUT.replace_all(&s, "");
    RE_IP.replace_all(&s, "<IP>").to_string()
}

fn build_notes_section(
    conn: &Connection,
    session_id: &str,
    limits: &BriefingLimits,
    out: &mut String,
) -> Result<(), Error> {
    let mut stmt = conn.prepare(
        "SELECT text FROM notes WHERE session_id = ?1 ORDER BY created_at DESC LIMIT ?2"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let notes: Vec<String> = stmt
        .query_map(params![session_id, limits.max_notes as i64], |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

    if notes.is_empty() {
        return Ok(());
    }

    out.push_str(&format!("## Notes (last {})\n", notes.len()));
    for text in &notes {
        out.push_str(text);
        out.push('\n');
    }

    Ok(())
}

fn build_commands_section(
    conn: &Connection,
    session_id: &str,
    limits: &BriefingLimits,
    out: &mut String,
) -> Result<(), Error> {
    let cmds = kb::list_history(conn, session_id, limits.max_commands * 3)?;
    if cmds.is_empty() {
        return Ok(());
    }

    let mut groups: Vec<(String, Vec<String>, Vec<i64>)> = Vec::new();
    for c in &cmds {
        let raw = c["command"].as_str().unwrap_or("");
        let exit_code = c["exit_code"].as_i64().unwrap_or(0);
        let normalized = normalize_command(raw);
        let target = RE_IP.find(raw).map(|m| m.as_str().to_string()).unwrap_or_default();

        if let Some(g) = groups.iter_mut().find(|(tmpl, _, _)| *tmpl == normalized) {
            if !target.is_empty() && !g.1.contains(&target) {
                g.1.push(target);
            }
            if !g.2.contains(&exit_code) {
                g.2.push(exit_code);
            }
        } else {
            let targets = if target.is_empty() { vec![] } else { vec![target] };
            groups.push((normalized, targets, vec![exit_code]));
        }
    }

    out.push_str("## Recent Commands\n");
    let shown = groups.len().min(limits.max_commands);
    for (tmpl, targets, exits) in &groups[..shown] {
        let count = cmds.iter().filter(|c| normalize_command(c["command"].as_str().unwrap_or("")) == *tmpl).count();
        let display = if tmpl.len() > 120 { &tmpl[..120] } else { tmpl };

        let mut line = display.to_string();
        if count > 1 {
            let target_str = if targets.is_empty() {
                String::new()
            } else {
                let short: Vec<&str> = targets.iter().map(|t| {
                    t.rsplit('.').next().unwrap_or(t)
                }).collect();
                format!(", targets: {}", short.join(", "))
            };
            line = format!("{display} (\u{00d7}{count}{target_str})");
        }

        let exit_str: Vec<String> = exits.iter().map(|e| e.to_string()).collect();
        out.push_str(&format!("{line} [exit={}]\n", exit_str.join(",")));
    }

    if groups.len() > shown {
        out.push_str(&format!("... +{} more\n", groups.len() - shown));
    }

    Ok(())
}

fn status_sort_key(status: &str) -> u8 {
    match status {
        "confirmed" => 0,
        "testing" => 1,
        "pending" => 2,
        "refuted" => 3,
        _ => 4,
    }
}

fn build_hypotheses_section(
    conn: &Connection,
    session_id: &str,
    limits: &BriefingLimits,
    out: &mut String,
) -> Result<(), Error> {
    let mut hyps = hypothesis::list(conn, session_id, None)?;
    if hyps.is_empty() {
        return Ok(());
    }

    hyps.sort_by_key(|h| status_sort_key(h["status"].as_str().unwrap_or("")));

    out.push_str("## Hypotheses\n");
    for h in &hyps {
        let id = h["id"].as_i64().unwrap_or(0);
        let status = h["status"].as_str().unwrap_or("pending").to_uppercase();
        let priority = h["priority"].as_str().unwrap_or("medium");
        let confidence = h["confidence"].as_f64().unwrap_or(0.5);
        let statement = h["statement"].as_str().unwrap_or("?");
        let category = h["category"].as_str().unwrap_or("");

        let cat_part = if category.is_empty() {
            String::new()
        } else {
            format!(" [{category}]")
        };

        if status == "REFUTED" {
            let evidence = hypothesis::list_evidence(conn, session_id, Some(id))?;
            out.push_str(&format!(
                "[{id}] REFUTED {statement}{cat_part} ({} evidence)\n",
                evidence.len()
            ));
            continue;
        }

        out.push_str(&format!(
            "[{id}] {status} ({priority}, {confidence}) {statement}{cat_part}\n"
        ));

        let evidence = hypothesis::list_evidence(conn, session_id, Some(id))?;
        let shown = evidence.len().min(limits.max_evidence_per_hyp);
        for e in &evidence[..shown] {
            let finding = e["finding"].as_str().unwrap_or("?");
            let severity = e["severity"].as_str().unwrap_or("info");
            out.push_str(&format!("  + \"{finding}\" [{severity}]\n"));
        }
        if evidence.len() > shown {
            out.push_str(&format!("  ... +{} more evidence\n", evidence.len() - shown));
        }
    }

    Ok(())
}

fn build_orphan_evidence_section(
    conn: &Connection,
    session_id: &str,
    out: &mut String,
) -> Result<(), Error> {
    let all_evidence = hypothesis::list_evidence(conn, session_id, None)?;
    let orphans: Vec<&serde_json::Value> = all_evidence
        .iter()
        .filter(|e| e["hypothesis_id"].is_null())
        .collect();

    if orphans.is_empty() {
        return Ok(());
    }

    out.push_str("## Unlinked Evidence\n");
    for e in orphans.iter().take(5) {
        let finding = e["finding"].as_str().unwrap_or("?");
        let severity = e["severity"].as_str().unwrap_or("info");
        out.push_str(&format!("  + \"{finding}\" [{severity}]\n"));
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

    fn insert_vuln(conn: &Connection, host_id: i64, port: i64, name: &str, severity: &str, cve: &str) {
        conn.execute(
            "INSERT INTO vulns (session_id, host_id, port, name, severity, cve) VALUES ('s1', ?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![host_id, port, name, severity, cve],
        ).unwrap();
    }

    #[test]
    fn briefing_vulns_severity_grouping() {
        let conn = setup();
        let hid = insert_host(&conn, "10.10.10.1");
        insert_vuln(&conn, hid, 80, "Apache HTTP Request Smuggling", "critical", "CVE-2023-25690");
        insert_vuln(&conn, hid, 80, "XSS-reflected", "medium", "");
        insert_vuln(&conn, hid, 80, "XSS-stored", "medium", "");

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Vulns"), "missing header");
        assert!(result.contains("critical"), "missing critical");
        assert!(result.contains("CVE-2023-25690"), "missing CVE");
        assert!(result.contains("Apache HTTP Request Smuggling"), "missing vuln name");
        assert!(result.contains("medium"), "missing medium count");
    }

    #[test]
    fn briefing_vulns_category_frequency_sort() {
        let conn = setup();
        let hid = insert_host(&conn, "10.10.10.1");
        for i in 0..5 {
            insert_vuln(&conn, hid, 80, &format!("XSS-variant{i}"), "medium", "");
        }
        for i in 0..3 {
            insert_vuln(&conn, hid, 80, &format!("info-disclosure{i}"), "low", "");
        }

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("5\u{00d7}XSS") || result.contains("5×XSS"), "missing XSS count");
        assert!(result.contains("3\u{00d7}info") || result.contains("3×info"), "missing info count");
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
    fn briefing_creds_access_flags() {
        let conn = setup();
        insert_host(&conn, "10.10.10.1");
        conn.execute(
            "INSERT INTO credentials (session_id, username, password, host, source) VALUES ('s1', 'admin', 'admin123', '10.10.10.1', 'hydra')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO access_levels (session_id, host, user, level, method) VALUES ('s1', '10.10.10.1', 'admin', 'user', 'ssh')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO flags (session_id, value, source) VALUES ('s1', 'HTB{abc123}', '/root/root.txt')",
            [],
        ).unwrap();

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Credentials"), "missing credentials header");
        assert!(result.contains("admin:admin123"), "missing cred");
        assert!(result.contains("10.10.10.1"), "missing host in cred");
        assert!(result.contains("hydra"), "missing source");
        assert!(result.contains("## Access"), "missing access header");
        assert!(result.contains("admin@10.10.10.1"), "missing access entry");
        assert!(result.contains("level=user"), "missing level");
        assert!(result.contains("method=ssh"), "missing method");
        assert!(result.contains("## Flags"), "missing flags header");
        assert!(result.contains("HTB{abc123}"), "missing flag value");
        assert!(result.contains("/root/root.txt"), "missing flag source");
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

    fn insert_hypothesis(conn: &Connection, statement: &str, category: &str, status: &str, priority: &str, confidence: f64) -> i64 {
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status, priority, confidence) VALUES ('s1', ?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![statement, category, status, priority, confidence],
        ).unwrap();
        conn.last_insert_rowid()
    }

    fn insert_evidence(conn: &Connection, hypothesis_id: Option<i64>, finding: &str, severity: &str) {
        conn.execute(
            "INSERT INTO evidence (session_id, hypothesis_id, finding, severity) VALUES ('s1', ?1, ?2, ?3)",
            rusqlite::params![hypothesis_id, finding, severity],
        ).unwrap();
    }

    #[test]
    fn briefing_hypotheses_with_evidence() {
        let conn = setup();
        let hid = insert_hypothesis(&conn, "Apache smuggling allows auth bypass", "Banner", "confirmed", "critical", 0.9);
        insert_evidence(&conn, Some(hid), "Smuggled request reached /admin with 200 OK", "high");

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Hypotheses"), "missing header");
        assert!(result.contains("CONFIRMED"), "missing CONFIRMED status");
        assert!(result.contains("Apache smuggling allows auth bypass"), "missing statement");
        assert!(result.contains("Smuggled request reached /admin with 200 OK"), "missing evidence finding");
    }

    #[test]
    fn briefing_refuted_hypotheses_collapsed() {
        let conn = setup();
        let hid = insert_hypothesis(&conn, "SSH brute force on root", "Session", "refuted", "high", 0.3);
        insert_evidence(&conn, Some(hid), "fail2ban blocks after 3 attempts", "info");
        insert_evidence(&conn, Some(hid), "root login disabled in sshd_config", "info");

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("REFUTED"), "missing REFUTED status");
        assert!(result.contains("(2 evidence)"), "missing evidence count");
        assert!(!result.contains("fail2ban"), "refuted should NOT show evidence body");
    }

    #[test]
    fn briefing_evidence_truncation() {
        let conn = setup();
        let hid = insert_hypothesis(&conn, "Multiple attack vectors", "Network", "confirmed", "high", 0.8);
        for i in 0..10 {
            insert_evidence(&conn, Some(hid), &format!("evidence item {i}"), "info");
        }

        let limits = BriefingLimits { max_evidence_per_hyp: 3, ..BriefingLimits::default() };
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("... +"), "missing truncation hint");
    }

    #[test]
    fn briefing_orphan_evidence() {
        let conn = setup();
        insert_evidence(&conn, None, "Anonymous FTP login successful", "medium");

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Unlinked Evidence"), "missing orphan evidence header");
        assert!(result.contains("Anonymous FTP login successful"), "missing orphan finding");
    }

    fn insert_note(conn: &Connection, text: &str) {
        conn.execute(
            "INSERT INTO notes (session_id, text) VALUES ('s1', ?1)",
            [text],
        ).unwrap();
    }

    fn insert_command(conn: &Connection, command: &str, exit_code: i64) {
        conn.execute(
            "INSERT INTO command_history (session_id, command, exit_code) VALUES ('s1', ?1, ?2)",
            rusqlite::params![command, exit_code],
        ).unwrap();
    }

    #[test]
    fn briefing_notes_ordering() {
        let conn = setup();
        conn.execute("INSERT INTO notes (session_id, text, created_at) VALUES ('s1', 'first note', '2025-01-01 00:00:01')", []).unwrap();
        conn.execute("INSERT INTO notes (session_id, text, created_at) VALUES ('s1', 'second note', '2025-01-01 00:00:02')", []).unwrap();
        conn.execute("INSERT INTO notes (session_id, text, created_at) VALUES ('s1', 'third note', '2025-01-01 00:00:03')", []).unwrap();

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Notes"), "missing notes header");
        let third_pos = result.find("third").expect("missing third");
        let first_pos = result.find("first").expect("missing first");
        assert!(third_pos < first_pos, "third note should appear before first (most recent first)");
    }

    #[test]
    fn briefing_commands_dedup() {
        let conn = setup();
        insert_command(&conn, "nmap -sV 10.10.10.1", 0);
        insert_command(&conn, "nmap -sV 10.10.10.2", 0);
        insert_command(&conn, "gobuster dir -u http://10.10.10.1", 0);

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(result.contains("## Recent Commands"), "missing commands header");
        assert!(result.contains("nmap"), "missing nmap");
        assert!(result.contains("gobuster"), "missing gobuster");
    }

    #[test]
    fn briefing_commands_normalization() {
        let conn = setup();
        insert_command(&conn, "nmap -sV 10.10.10.1", 0);
        insert_command(&conn, "nmap -sV 10.10.10.2", 0);
        insert_command(&conn, "nmap -sV 10.10.10.3", 0);

        let limits = BriefingLimits::default();
        let result = build_briefing_with_limits(&conn, "s1", &limits).unwrap();
        assert!(
            result.contains("\u{00d7}3") || result.contains("×3"),
            "missing count indicator for 3 deduped nmap commands: {result}"
        );
    }
}
