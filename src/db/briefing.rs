use crate::db::kb;
use crate::error::Error;
use rusqlite::Connection;

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
