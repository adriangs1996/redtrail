use crate::db::{commands, dispatcher, session};
use crate::error::Error;
use crate::net;
use rusqlite::Connection;
use std::collections::HashMap;

pub struct CommandResult {
    pub command_id: i64,
    pub flags_found: Vec<String>,
    pub scope_warnings: Vec<String>,
}

pub fn process_command(
    conn: &Connection,
    session_id: &str,
    command: &str,
    exit_code: i32,
    duration_ms: i64,
    output: &str,
    tool: Option<&str>,
) -> Result<CommandResult, Error> {
    let cmd_id = commands::insert(conn, session_id, command, tool)?;
    commands::finish(conn, cmd_id, exit_code, duration_ms, output)?;

    let mut flags_found = Vec::new();
    if let Ok(patterns) = session::load_flag_patterns(conn, session_id) {
        for pat in &patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                for m in re.find_iter(output) {
                    let flag = m.as_str().to_string();
                    let mut data = HashMap::new();
                    data.insert("value".to_string(), serde_json::Value::String(flag.clone()));
                    data.insert(
                        "source".to_string(),
                        serde_json::Value::String(command.to_string()),
                    );
                    let _ = dispatcher::create(conn, session_id, "flags", &data);
                    flags_found.push(flag);
                }
            }
        }
    }

    if let Some(t) = tool {
        let cost = detection_cost(t);
        if cost > 0.0 {
            let _ = session::decrement_noise_budget(conn, session_id, cost);
        }
    }

    let mut scope_warnings = Vec::new();
    if let Ok(Some(scope)) = session::load_scope(conn, session_id) {
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
    use crate::db;

    fn setup() -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        session::create_session(
            &conn,
            "s1",
            "test",
            "/tmp/test",
            Some("10.10.10.1"),
            Some("10.10.10.0/24"),
            "general",
        )
        .unwrap();
        (conn, "s1".to_string())
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
        let (conn, sid) = setup();
        let result = process_command(
            &conn,
            &sid,
            "nmap -sV 10.10.10.1",
            0,
            1500,
            "22/tcp open ssh",
            Some("nmap"),
        )
        .unwrap();

        assert!(result.command_id > 0);
    }

    #[test]
    fn test_process_command_captures_flags() {
        let (conn, sid) = setup();
        let output = "user.txt: HTB{fake_user_flag}\nroot.txt: HTB{fake_root_flag}";
        let result = process_command(
            &conn,
            &sid,
            "cat user.txt root.txt",
            0,
            100,
            output,
            Some("cat"),
        )
        .unwrap();

        assert_eq!(result.flags_found.len(), 2);
        assert!(result.flags_found.contains(&"HTB{fake_user_flag}".to_string()));
        assert!(result.flags_found.contains(&"HTB{fake_root_flag}".to_string()));
    }

    #[test]
    fn test_process_command_scope_warnings() {
        let (conn, sid) = setup();
        let result =
            process_command(&conn, &sid, "nmap -sV 192.168.1.1", 0, 500, "", Some("nmap"))
                .unwrap();

        assert_eq!(result.scope_warnings.len(), 1);
        assert!(result.scope_warnings[0].contains("192.168.1.1"));
        assert!(result.scope_warnings[0].contains("out of scope"));
    }

    #[test]
    fn test_process_command_decrements_noise_budget() {
        let (conn, sid) = setup();

        let summary = session::status_summary(&conn, &sid).unwrap();
        let budget_before = summary["noise_budget"].as_f64().unwrap();
        assert_eq!(budget_before, 1.0);

        process_command(&conn, &sid, "nmap 10.10.10.1", 0, 500, "", Some("nmap")).unwrap();

        let summary = session::status_summary(&conn, &sid).unwrap();
        let budget_after = summary["noise_budget"].as_f64().unwrap();
        assert!(
            (budget_after - 0.8).abs() < 0.001,
            "nmap costs 0.2, budget should be 0.8, got {budget_after}"
        );
    }
}
