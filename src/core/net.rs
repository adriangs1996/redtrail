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
