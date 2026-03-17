use regex::Regex;
use crate::db::Db;

pub struct PipelineResult {
    pub flags_found: Vec<String>,
    pub scope_warnings: Vec<String>,
}

pub fn post_exec(
    db: &Db,
    session_id: &str,
    command: &str,
    output: &str,
    tool: Option<&str>,
) -> PipelineResult {
    let mut result = PipelineResult {
        flags_found: Vec::new(),
        scope_warnings: Vec::new(),
    };

    if let Ok(patterns) = load_flag_patterns(db, session_id) {
        for pat in &patterns {
            if let Ok(re) = Regex::new(pat) {
                for m in re.find_iter(output) {
                    let flag = m.as_str().to_string();
                    let _ = db.add_flag(session_id, &flag, Some(command));
                    result.flags_found.push(flag);
                }
            }
        }
    }

    if let Ok(Some(scope)) = load_scope(db, session_id) {
        let ips = extract_ips(command);
        for ip in &ips {
            if !ip_in_scope(ip, &scope) {
                result.scope_warnings.push(format!("{ip} is out of scope ({scope})"));
            }
        }
    }

    if let Some(tool) = tool {
        let cost = detection_cost(tool);
        if cost > 0.0 {
            let _ = db.decrement_noise_budget(session_id, cost);
        }
    }

    result
}

fn load_flag_patterns(db: &Db, session_id: &str) -> Result<Vec<String>, crate::error::Error> {
    let meta: Option<String> = db.conn().query_row(
        "SELECT goal_meta FROM sessions WHERE id = ?1",
        rusqlite::params![session_id],
        |r| r.get(0),
    ).map_err(|e| crate::error::Error::Db(e.to_string()))?;

    if let Some(m) = meta {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&m) {
            if let Some(arr) = v.get("flag_patterns").and_then(|x| x.as_array()) {
                let pats: Vec<String> = arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect();
                if !pats.is_empty() {
                    return Ok(pats);
                }
            }
        }
    }

    Ok(vec![
        r"HTB\{[^}]+\}".to_string(),
        r"FLAG\{[^}]+\}".to_string(),
        r"flag\{[^}]+\}".to_string(),
    ])
}

fn load_scope(db: &Db, session_id: &str) -> Result<Option<String>, crate::error::Error> {
    let scope: Option<String> = db.conn().query_row(
        "SELECT scope FROM sessions WHERE id = ?1",
        rusqlite::params![session_id],
        |r| r.get(0),
    ).map_err(|e| crate::error::Error::Db(e.to_string()))?;
    Ok(scope.filter(|s| !s.is_empty()))
}

fn extract_ips(command: &str) -> Vec<String> {
    let re = Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").unwrap();
    re.find_iter(command)
        .map(|m| m.as_str().to_string())
        .filter(|ip| {
            ip.split('.').all(|o| o.parse::<u8>().is_ok())
        })
        .collect()
}

fn ip_to_u32(ip: &str) -> Option<u32> {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 { return None; }
    let mut n: u32 = 0;
    for p in parts {
        let octet: u32 = p.parse().ok()?;
        if octet > 255 { return None; }
        n = (n << 8) | octet;
    }
    Some(n)
}

fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    let parts: Vec<&str> = cidr.splitn(2, '/').collect();
    if parts.len() != 2 { return false; }
    let prefix_len: u32 = match parts[1].parse() {
        Ok(n) if n <= 32 => n,
        _ => return false,
    };
    let base = match ip_to_u32(parts[0]) {
        Some(n) => n,
        None => return false,
    };
    let target = match ip_to_u32(ip) {
        Some(n) => n,
        None => return false,
    };
    let mask = if prefix_len == 0 { 0u32 } else { !0u32 << (32 - prefix_len) };
    (target & mask) == (base & mask)
}

fn ip_in_scope(ip: &str, scope: &str) -> bool {
    scope.split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .any(|cidr| ip_in_cidr(ip, cidr))
}

fn detection_cost(tool: &str) -> f64 {
    match tool {
        "nmap" | "masscan" => 0.2,
        "gobuster" | "ffuf" | "dirb" | "feroxbuster" | "wfuzz" => 0.3,
        "sqlmap" | "nuclei" => 0.5,
        "hydra" | "john" | "hashcat" | "crackmapexec" => 0.8,
        _ => 0.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ips() {
        let ips = extract_ips("nmap -sV 10.10.10.1 -p 22");
        assert!(ips.contains(&"10.10.10.1".to_string()));
    }

    #[test]
    fn test_detection_cost() {
        assert_eq!(detection_cost("nmap"), 0.2);
        assert_eq!(detection_cost("sqlmap"), 0.5);
        assert_eq!(detection_cost("hydra"), 0.8);
        assert_eq!(detection_cost("curl"), 0.1);
    }

    #[test]
    fn test_ip_in_scope() {
        assert!(ip_in_scope("10.10.10.5", "10.10.10.0/24"));
        assert!(!ip_in_scope("192.168.1.1", "10.10.10.0/24"));
        assert!(ip_in_scope("10.10.10.5", "10.10.10.0/24, 192.168.0.0/16"));
    }

    #[test]
    fn test_extract_ips_multiple() {
        let ips = extract_ips("nmap 10.10.10.1 10.10.10.2");
        assert!(ips.contains(&"10.10.10.1".to_string()));
        assert!(ips.contains(&"10.10.10.2".to_string()));
    }
}
