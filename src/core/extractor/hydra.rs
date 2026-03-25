use super::{SynthesisResult, Fact, Relation, Synthetizer};
use regex::Regex;

fn extract(_command: &str, output: &str) -> SynthesisResult {
    let mut facts = Vec::new();
    let mut relations = Vec::new();

    let re = Regex::new(
        r"\[(\d+)\]\[(\w+)\]\s+host:\s*(\S+)\s+login:\s*(\S+)\s+password:\s*(.+)"
    ).unwrap();

    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            let port: u16 = caps[1].parse().unwrap_or(0);
            let service = caps[2].to_string();
            let ip = caps[3].to_string();
            let username = caps[4].to_string();
            let password = caps[5].trim().to_string();

            let key = format!("credential:{username}:{service}:{ip}");
            facts.push(Fact {
                fact_type: "credential".into(),
                key: key.clone(),
                attributes: serde_json::json!({
                    "ip": ip,
                    "port": port,
                    "service": service,
                    "username": username,
                    "password": password,
                }),
            });

            relations.push(Relation {
                from_key: key,
                to_key: format!("service:{ip}:{port}/tcp"),
                relation_type: "authenticates_to".into(),
            });
        }
    }

    SynthesisResult { facts, relations }
}

fn runs_on(tool: &str) -> bool {
    tool == "hydra"
}

inventory::submit! {
    Synthetizer::new(runs_on, extract)
}
