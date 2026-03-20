use crate::db::{CommandLog, KnowledgeBase, SessionOps};
use crate::error::Error;
use crate::net;

pub struct CommandResult {
    pub command_id: i64,
    pub flags_found: Vec<String>,
    pub scope_warnings: Vec<String>,
}

pub fn process_command(
    db: &(impl CommandLog + KnowledgeBase + SessionOps),
    session_id: &str,
    command: &str,
    exit_code: i32,
    duration_ms: i64,
    output: &str,
    tool: Option<&str>,
) -> Result<CommandResult, Error> {
    let cmd_id = db.insert_command(session_id, command, tool)?;
    db.finish_command(cmd_id, exit_code, duration_ms, output)?;

    let mut flags_found = Vec::new();
    if let Ok(patterns) = db.load_flag_patterns(session_id) {
        for pat in &patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                for m in re.find_iter(output) {
                    let flag = m.as_str().to_string();
                    let _ = db.add_flag(session_id, &flag, Some(command));
                    flags_found.push(flag);
                }
            }
        }
    }

    if let Some(t) = tool {
        let cost = detection_cost(t);
        if cost > 0.0 {
            let _ = db.decrement_noise_budget(session_id, cost);
        }
    }

    let mut scope_warnings = Vec::new();
    if let Ok(Some(scope)) = db.load_scope(session_id) {
        for ip in &net::extract_ips(command) {
            if !net::ip_in_scope(ip, &scope) {
                scope_warnings.push(format!("{ip} is out of scope ({scope})"));
            }
        }
    }

    Ok(CommandResult {
        command_id: cmd_id,
        flags_found,
        scope_warnings,
    })
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
    use crate::db::{SessionOps, open_in_memory};

    fn setup() -> (impl CommandLog + KnowledgeBase + SessionOps, String) {
        let db = open_in_memory().unwrap();
        db.create_session(
            "s1",
            "test",
            Some("10.10.10.1"),
            Some("10.10.10.0/24"),
            "general",
        )
        .unwrap();
        (db, "s1".to_string())
    }

    #[test]
    fn test_detection_cost() {
        assert_eq!(detection_cost("nmap"), 0.2);
        assert_eq!(detection_cost("sqlmap"), 0.5);
        assert_eq!(detection_cost("hydra"), 0.8);
        assert_eq!(detection_cost("curl"), 0.1);
    }

    #[test]
    fn test_process_command_records_and_finishes() {
        let (db, sid) = setup();
        let result = process_command(
            &db,
            &sid,
            "nmap -sV 10.10.10.1",
            0,
            1500,
            "22/tcp open ssh",
            Some("nmap"),
        )
        .unwrap();

        assert!(result.command_id > 0);

        let history = db.list_history(&sid, 1).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0]["command"], "nmap -sV 10.10.10.1");
        assert_eq!(history[0]["exit_code"], 0);
    }

    #[test]
    fn test_process_command_captures_flags() {
        let (db, sid) = setup();
        let output = "user.txt: HTB{fake_user_flag}\nroot.txt: HTB{fake_root_flag}";
        let result = process_command(
            &db,
            &sid,
            "cat user.txt root.txt",
            0,
            100,
            output,
            Some("cat"),
        )
        .unwrap();

        assert_eq!(result.flags_found.len(), 2);
        assert!(
            result
                .flags_found
                .contains(&"HTB{fake_user_flag}".to_string())
        );
        assert!(
            result
                .flags_found
                .contains(&"HTB{fake_root_flag}".to_string())
        );

        let flags = db.list_flags(&sid).unwrap();
        assert_eq!(flags.len(), 2);
    }

    #[test]
    fn test_process_command_scope_warnings() {
        let (db, sid) = setup();
        let result =
            process_command(&db, &sid, "nmap -sV 192.168.1.1", 0, 500, "", Some("nmap")).unwrap();

        assert_eq!(result.scope_warnings.len(), 1);
        assert!(result.scope_warnings[0].contains("192.168.1.1"));
        assert!(result.scope_warnings[0].contains("out of scope"));
    }

    #[test]
    fn test_process_command_decrements_noise_budget() {
        let (db, sid) = setup();

        let summary = db.status_summary(&sid).unwrap();
        let budget_before = summary["noise_budget"].as_f64().unwrap();
        assert_eq!(budget_before, 1.0);

        process_command(&db, &sid, "nmap 10.10.10.1", 0, 500, "", Some("nmap")).unwrap();

        let summary = db.status_summary(&sid).unwrap();
        let budget_after = summary["noise_budget"].as_f64().unwrap();
        assert!(
            (budget_after - 0.8).abs() < 0.001,
            "nmap costs 0.2, budget should be 0.8, got {budget_after}"
        );
    }
}
