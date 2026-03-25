use super::{SynthesisResult, Fact, Relation, Synthetizer};
use regex::Regex;

fn parse_target_from_command(command: &str) -> (String, u16) {
    let re = Regex::new(r"https?://([^/:]+)(?::(\d+))?").unwrap();
    if let Some(caps) = re.captures(command) {
        let ip = caps[1].to_string();
        let port = caps.get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(if command.contains("https://") { 443 } else { 80 });
        return (ip, port);
    }
    ("unknown".into(), 80)
}

fn extract(command: &str, output: &str) -> SynthesisResult {
    let mut facts = Vec::new();
    let mut relations = Vec::new();

    let (target_ip, target_port) = parse_target_from_command(command);

    // Gobuster: /path (Status: 200) [Size: 1234] [--> redirect]
    let re_gobuster = Regex::new(
        r"(/\S+)\s+\(Status:\s*(\d+)\)\s*\[Size:\s*(\d+)\](?:\s*\[--> ([^\]]+)\])?"
    ).unwrap();

    // ffuf: path [Status: 200, Size: 1234, Words: N, Lines: N]
    let re_ffuf = Regex::new(
        r"^(\S+)\s+\[Status:\s*(\d+),\s*Size:\s*(\d+)"
    ).unwrap();

    for line in output.lines() {
        let (path, status, size, redirect) = if let Some(caps) = re_gobuster.captures(line) {
            (
                caps[1].to_string(),
                caps[2].parse::<u16>().unwrap_or(0),
                caps[3].parse::<u64>().unwrap_or(0),
                caps.get(4).map(|m| m.as_str().to_string()),
            )
        } else if let Some(caps) = re_ffuf.captures(line) {
            (
                format!("/{}", &caps[1]),
                caps[2].parse::<u16>().unwrap_or(0),
                caps[3].parse::<u64>().unwrap_or(0),
                None,
            )
        } else {
            continue;
        };

        let key = format!("web_path:{target_ip}:{target_port}:{path}");
        let mut attrs = serde_json::json!({
            "ip": target_ip,
            "port": target_port,
            "path": path,
            "status_code": status,
            "content_length": size,
        });
        if let Some(ref redir) = redirect {
            attrs["redirect_to"] = serde_json::json!(redir);
        }

        facts.push(Fact {
            fact_type: "web_path".into(),
            key: key.clone(),
            attributes: attrs,
        });

        relations.push(Relation {
            from_key: key,
            to_key: format!("service:{target_ip}:{target_port}/tcp"),
            relation_type: "served_by".into(),
        });
    }

    SynthesisResult { facts, relations }
}

fn runs_on(tool: &str) -> bool {
    matches!(tool, "gobuster" | "ffuf" | "feroxbuster" | "dirb" | "wfuzz")
}

inventory::submit! {
    Synthetizer::new(runs_on, extract)
}
