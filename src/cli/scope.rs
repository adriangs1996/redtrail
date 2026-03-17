use clap::Subcommand;
use crate::error::Error;
use super::resolve_session;

#[derive(Subcommand)]
pub enum ScopeCommands {
    Check {
        ip: String,
    },
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

pub fn run(cmd: ScopeCommands) -> Result<(), Error> {
    match cmd {
        ScopeCommands::Check { ip } => {
            let (db, session_id) = resolve_session()?;
            let scope: Option<String> = db.conn().query_row(
                "SELECT scope FROM sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |r| r.get(0),
            ).map_err(|e| Error::Db(e.to_string()))?;

            let in_scope = match scope.as_deref() {
                None | Some("") => true,
                Some(s) => s.split(',')
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .any(|cidr| ip_in_cidr(&ip, cidr)),
            };

            if in_scope {
                println!("in-scope");
                Ok(())
            } else {
                println!("out-of-scope");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_in_cidr() {
        assert!(ip_in_cidr("10.10.10.5", "10.10.10.0/24"));
        assert!(ip_in_cidr("10.10.10.0", "10.10.10.0/24"));
        assert!(ip_in_cidr("10.10.10.255", "10.10.10.0/24"));
        assert!(!ip_in_cidr("10.10.11.1", "10.10.10.0/24"));
        assert!(!ip_in_cidr("192.168.1.1", "10.10.10.0/24"));
        assert!(ip_in_cidr("10.0.0.1", "0.0.0.0/0"));
        assert!(ip_in_cidr("1.2.3.4", "1.2.3.4/32"));
        assert!(!ip_in_cidr("1.2.3.5", "1.2.3.4/32"));
    }
}
