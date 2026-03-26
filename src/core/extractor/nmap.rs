use super::{Fact, Relation, SynthesisResult, Synthetizer};
use regex::Regex;

fn extract(_command: &str, output: &str) -> SynthesisResult {
    let mut facts = Vec::new();
    let mut relations = Vec::new();
    let mut current_host: Option<String> = None;

    let re_host = Regex::new(r"Nmap scan report for (?:(\S+) \()?(\d+\.\d+\.\d+\.\d+)\)?").unwrap();
    let re_port =
        Regex::new(r"(\d+)/(tcp|udp)\s+(open|closed|filtered)\s+(\S+)(?:\s+(.+))?").unwrap();
    let re_os = Regex::new(r"OS details?:\s*(.+)").unwrap();

    for line in output.lines() {
        if let Some(caps) = re_host.captures(line) {
            let ip = caps.get(2).unwrap().as_str().to_string();
            let hostname = caps.get(1).map(|m| m.as_str().to_string());

            let mut attrs = serde_json::json!({"ip": &ip, "status": "up"});
            if let Some(ref h) = hostname {
                attrs["hostname"] = serde_json::json!(h);
            }

            facts.push(Fact {
                fact_type: "host".into(),
                key: format!("host:{ip}"),
                attributes: attrs,
            });
            current_host = Some(ip);
        }

        if let (Some(host), Some(caps)) = (&current_host, re_port.captures(line)) {
            let port: u16 = match caps[1].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let proto = &caps[2];
            let state = &caps[3];
            let service = &caps[4];
            let version = caps.get(5).map(|m| m.as_str().trim().to_string());

            if state != "open" {
                continue;
            }

            let mut attrs = serde_json::json!({
                "ip": host,
                "port": port,
                "protocol": proto,
                "service": service,
            });
            if let Some(ref v) = version
                && !v.is_empty()
            {
                attrs["version"] = serde_json::json!(v);
            }

            let key = format!("service:{host}:{port}/{proto}");
            facts.push(Fact {
                fact_type: "service".into(),
                key: key.clone(),
                attributes: attrs,
            });
            relations.push(Relation {
                from_key: key,
                to_key: format!("host:{host}"),
                relation_type: "runs_on".into(),
            });
        }

        if let Some(caps) = re_os.captures(line)
            && let Some(ref host) = current_host
        {
            let os = caps[1].trim().to_string();
            let key = format!("os:{host}");
            facts.push(Fact {
                fact_type: "os_info".into(),
                key: key.clone(),
                attributes: serde_json::json!({"ip": host, "os": os}),
            });
            relations.push(Relation {
                from_key: key,
                to_key: format!("host:{host}"),
                relation_type: "describes".into(),
            });
        }
    }

    SynthesisResult { facts, relations }
}

fn runs_on(tool: &str) -> bool {
    tool == "nmap"
}

inventory::submit! {
    Synthetizer::new(runs_on, extract)
}
