use crate::db::Db;
use crate::net;

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

    if let Ok(patterns) = db.load_flag_patterns(session_id) {
        for pat in &patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                for m in re.find_iter(output) {
                    let flag = m.as_str().to_string();
                    let _ = db.add_flag(session_id, &flag, Some(command));
                    result.flags_found.push(flag);
                }
            }
        }
    }

    if let Ok(Some(scope)) = db.load_scope(session_id) {
        for ip in &net::extract_ips(command) {
            if !net::ip_in_scope(ip, &scope) {
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
    fn test_detection_cost() {
        assert_eq!(detection_cost("nmap"), 0.2);
        assert_eq!(detection_cost("sqlmap"), 0.5);
        assert_eq!(detection_cost("hydra"), 0.8);
        assert_eq!(detection_cost("curl"), 0.1);
    }
}
