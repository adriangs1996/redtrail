fn ip_to_u32(ip: &str) -> Option<u32> {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut n: u32 = 0;
    for p in parts {
        let octet: u32 = p.parse().ok()?;
        if octet > 255 {
            return None;
        }
        n = (n << 8) | octet;
    }
    Some(n)
}

pub fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    let parts: Vec<&str> = cidr.splitn(2, '/').collect();
    if parts.len() != 2 {
        return false;
    }
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
    let mask = if prefix_len == 0 {
        0u32
    } else {
        !0u32 << (32 - prefix_len)
    };
    (target & mask) == (base & mask)
}

pub fn extract_ips(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").unwrap();
    re.find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|ip| ip.split('.').all(|o| o.parse::<u8>().is_ok()))
        .collect()
}

pub fn ip_in_scope(ip: &str, scope: &str) -> bool {
    scope
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .any(|cidr| ip_in_cidr(ip, cidr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cidr_boundaries() {
        assert!(ip_in_cidr("10.10.10.0", "10.10.10.0/24"));
        assert!(ip_in_cidr("10.10.10.5", "10.10.10.0/24"));
        assert!(ip_in_cidr("10.10.10.255", "10.10.10.0/24"));
        assert!(!ip_in_cidr("10.10.11.1", "10.10.10.0/24"));
        assert!(!ip_in_cidr("192.168.1.1", "10.10.10.0/24"));
        assert!(ip_in_cidr("1.2.3.4", "1.2.3.4/32"));
        assert!(!ip_in_cidr("1.2.3.5", "1.2.3.4/32"));
        assert!(ip_in_cidr("10.0.0.1", "0.0.0.0/0"));
    }

    #[test]
    fn test_scope_csv() {
        assert!(ip_in_scope("10.10.10.5", "10.10.10.0/24"));
        assert!(!ip_in_scope("192.168.1.1", "10.10.10.0/24"));
        assert!(ip_in_scope("192.168.1.1", "10.10.10.0/24, 192.168.0.0/16"));
        assert!(!ip_in_scope("172.16.0.1", "10.10.10.0/24, 192.168.0.0/16"));
        assert!(!ip_in_scope("10.10.10.5", ""));
        assert!(!ip_in_scope("10.10.10.5", "  , "));
    }

    #[test]
    fn test_extract_ips() {
        let ips = extract_ips("nmap -sV 10.10.10.1 -p 22");
        assert_eq!(ips, vec!["10.10.10.1"]);

        let ips = extract_ips("nmap 10.10.10.1 10.10.10.2");
        assert_eq!(ips.len(), 2);
        assert!(ips.contains(&"10.10.10.1".to_string()));
        assert!(ips.contains(&"10.10.10.2".to_string()));

        assert!(extract_ips("no ips here").is_empty());
        assert!(extract_ips("999.999.999.999").is_empty());
    }
}
