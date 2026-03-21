use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::db;
use crate::db::schema;
use crate::error::Error;
use crate::skill_loader;
use super::{Agent, ToolContext};
use super::tools::*;
use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::LanguageModel;

pub const MAX_ASSISTANT_ROUNDS: usize = 20;

pub fn build_system_prompt(
    conn: &Connection,
    session_id: &str,
    cwd: &Path,
    skill_override: Option<&str>,
    no_skill: bool,
) -> Result<String, Error> {
    let skill_content: Option<(String, String)> = if no_skill {
        None
    } else if let Some(name) = skill_override {
        let prompt = skill_loader::load_skill_prompt(name, Some(cwd))?;
        eprintln!("[skill] loading {name} (manual override)");
        Some((name.to_string(), prompt))
    } else {
        match skill_loader::detect_phase(conn, session_id)? {
            Some(m) => match skill_loader::load_skill_prompt(&m.skill_name, Some(cwd)) {
                Ok(prompt) => {
                    eprintln!(
                        "[phase] {} ({}) — loading {}",
                        m.phase_name, m.context, m.skill_name
                    );
                    Some((m.skill_name, prompt))
                }
                Err(_) => {
                    eprintln!(
                        "[phase] {} ({}) — skill {} not found, using generic",
                        m.phase_name, m.context, m.skill_name
                    );
                    None
                }
            },
            None => None,
        }
    };

    let session = db::session::get_session(conn, session_id)?;
    let summary = db::session::status_summary(conn, session_id)?;
    let hosts = db::kb::list_hosts(conn, session_id)?;
    let ports = db::kb::list_ports(conn, session_id, None)?;
    let creds = db::kb::list_credentials(conn, session_id)?;
    let flags = db::kb::list_flags(conn, session_id)?;
    let access = db::kb::list_access(conn, session_id)?;
    let notes = db::kb::list_notes(conn, session_id)?;
    let history = db::kb::list_history(conn, session_id, 30)?;
    let hypotheses = db::hypothesis::list(conn, session_id, None)?;

    let db_schema = schema::as_json(conn);
    let schema_str = serde_json::to_string_pretty(&db_schema).unwrap_or_default();

    let target = session["target"].as_str().unwrap_or("(none)");
    let scope = session["scope"].as_str().unwrap_or("(unrestricted)");
    let goal = session["goal"].as_str().unwrap_or("general");
    let phase = summary["phase"].as_str().unwrap_or("L0");
    let noise = summary["noise_budget"].as_f64().unwrap_or(1.0);

    let mut p = String::with_capacity(8192);

    if let Some((skill_name, skill_prompt)) = &skill_content {
        p.push_str(&format!("Active skill: {skill_name}\n---\n"));
        p.push_str(skill_prompt);
        p.push_str("\n---\n\n");
    } else {
        p.push_str("You are Redtrail, a pentesting advisor embedded in a workspace. You help the operator by analyzing data, suggesting next steps, running commands, and querying the knowledge base.\n\n");
        p.push_str("Be concise and direct. Use pentesting terminology. When suggesting commands, prefer the tools already aliased in the workspace.\n\n");
    }

    p.push_str(&format!("Target: {target}\nScope: {scope}\nGoal: {goal}\n"));
    if skill_content.is_none() {
        p.push_str(&format!("Phase: {phase}\n"));
    }
    p.push_str(&format!(
        "Noise budget: {noise:.2}\nCWD: {}\n\n",
        cwd.display()
    ));

    if !hosts.is_empty() {
        p.push_str("=== Hosts ===\n");
        for h in &hosts {
            p.push_str(&format!(
                "  {} {} {}\n",
                h["ip"].as_str().unwrap_or(""),
                h["hostname"].as_str().unwrap_or("-"),
                h["os"].as_str().unwrap_or("-"),
            ));
        }
        p.push('\n');
    }

    if !ports.is_empty() {
        p.push_str("=== Ports ===\n");
        for port in &ports {
            p.push_str(&format!(
                "  {}:{}/{} {} {}\n",
                port["ip"].as_str().unwrap_or(""),
                port["port"].as_i64().unwrap_or(0),
                port["protocol"].as_str().unwrap_or("tcp"),
                port["service"].as_str().unwrap_or("-"),
                port["version"].as_str().unwrap_or(""),
            ));
        }
        p.push('\n');
    }

    if !creds.is_empty() {
        p.push_str("=== Credentials ===\n");
        for c in &creds {
            p.push_str(&format!(
                "  {}:{} @ {} ({})\n",
                c["username"].as_str().unwrap_or(""),
                c["password"].as_str().unwrap_or("***"),
                c["host"].as_str().unwrap_or("-"),
                c["source"].as_str().unwrap_or("-"),
            ));
        }
        p.push('\n');
    }

    if !flags.is_empty() {
        p.push_str("=== Flags ===\n");
        for f in &flags {
            p.push_str(&format!(
                "  {} ({})\n",
                f["value"].as_str().unwrap_or(""),
                f["source"].as_str().unwrap_or("-"),
            ));
        }
        p.push('\n');
    }

    if !access.is_empty() {
        p.push_str("=== Access ===\n");
        for a in &access {
            p.push_str(&format!(
                "  {}@{} level={} method={}\n",
                a["user"].as_str().unwrap_or(""),
                a["host"].as_str().unwrap_or(""),
                a["level"].as_str().unwrap_or(""),
                a["method"].as_str().unwrap_or("-"),
            ));
        }
        p.push('\n');
    }

    if !hypotheses.is_empty() {
        p.push_str("=== Hypotheses ===\n");
        for h in &hypotheses {
            p.push_str(&format!(
                "  [{}] {} — {} (priority={}, conf={:.1})\n",
                h["id"],
                h["statement"].as_str().unwrap_or(""),
                h["status"].as_str().unwrap_or(""),
                h["priority"].as_str().unwrap_or(""),
                h["confidence"].as_f64().unwrap_or(0.0),
            ));
        }
        p.push('\n');
    }

    if !notes.is_empty() {
        p.push_str("=== Notes ===\n");
        for n in notes.iter().rev().take(10) {
            p.push_str(&format!("  {}\n", n["text"].as_str().unwrap_or("")));
        }
        p.push('\n');
    }

    if !history.is_empty() {
        p.push_str("=== Recent Commands ===\n");
        for h in history.iter().rev().take(20) {
            let exit = h["exit_code"]
                .as_i64()
                .map(|c| c.to_string())
                .unwrap_or("-".to_string());
            p.push_str(&format!(
                "  [exit={}] {}\n",
                exit,
                h["command"].as_str().unwrap_or(""),
            ));
        }
        p.push('\n');
    }

    p.push_str("## Database Schema (for query_table, create_record, update_record tools)\n");
    p.push_str(&schema_str);
    p.push_str("\n\n");

    p.push_str("## Tools\n");
    p.push_str("You have 6 tools:\n");
    p.push_str("- query_table: query rows from the knowledge base with optional filters\n");
    p.push_str("- create_record: insert new records (supports ip-to-host_id resolution for ports/web_paths/vulns)\n");
    p.push_str("- update_record: update existing records by id\n");
    p.push_str("- suggest: suggest a next action to the operator (include priority: low/medium/high/critical)\n");
    p.push_str("- respond: send a response message to the operator\n");
    p.push_str("- run_command: execute shell commands in the workspace\n\n");

    p.push_str(&format!("You have a maximum of {MAX_ASSISTANT_ROUNDS} tool-calling rounds. Be efficient.\n"));
    p.push_str("Use respond to send your final answer. Use suggest for actionable next steps.\n");
    p.push_str("Always explain what you're doing. Be concise.");

    Ok(p)
}

pub fn build_assistant_agent<M: LanguageModel + TextInputSupport + ToolCallSupport>(
    model: M,
    conn: Arc<Mutex<Connection>>,
    session_id: String,
    cwd: PathBuf,
    skill_override: Option<&str>,
    no_skill: bool,
) -> Result<Agent<M>, Error> {
    let (system, active_skill_name) = {
        let c = conn.lock().map_err(|e| Error::Config(format!("db lock: {e}")))?;
        let prompt = build_system_prompt(&c, &session_id, &cwd, skill_override, no_skill)?;
        let skill_name = resolve_active_skill_name(&c, &session_id, &cwd, skill_override, no_skill);
        (prompt, skill_name)
    };

    let skill_tools = active_skill_name
        .map(|name| skill_loader::load_skill_config(&name, Some(&cwd)))
        .and_then(|cfg| cfg.tools);

    let ctx = Arc::new(ToolContext { conn, session_id, cwd });
    let all_tools = vec![
        make_query_tool(ctx.clone()),
        make_create_tool(ctx.clone()),
        make_update_tool(ctx.clone()),
        make_run_command_tool(ctx),
        make_suggest_tool(),
        make_respond_tool(),
    ];

    let tools = match skill_tools {
        Some(allowed) => all_tools
            .into_iter()
            .filter(|t| allowed.iter().any(|a| a == &t.name))
            .collect(),
        None => all_tools,
    };

    Ok(Agent::new(model, system, tools, MAX_ASSISTANT_ROUNDS))
}

fn resolve_active_skill_name(
    conn: &Connection,
    session_id: &str,
    cwd: &Path,
    skill_override: Option<&str>,
    no_skill: bool,
) -> Option<String> {
    if no_skill {
        return None;
    }
    if let Some(name) = skill_override {
        return Some(name.to_string());
    }
    skill_loader::detect_phase(conn, session_id)
        .ok()
        .flatten()
        .and_then(|m| {
            if skill_loader::load_skill_prompt(&m.skill_name, Some(cwd)).is_ok() {
                Some(m.skill_name)
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_conn() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, target, scope, goal) VALUES ('s1', 'test', '10.10.10.1', '10.10.10.0/24', 'ctf')",
            [],
        ).unwrap();
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn system_prompt_contains_session_context() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("Target: 10.10.10.1"));
        assert!(prompt.contains("Scope: 10.10.10.0/24"));
        assert!(prompt.contains("Goal: ctf"));
    }

    #[test]
    fn system_prompt_contains_db_schema() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("Database Schema"));
        assert!(prompt.contains("hosts"));
        assert!(prompt.contains("ports"));
    }

    #[test]
    fn system_prompt_contains_tool_descriptions() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("query_table"));
        assert!(prompt.contains("create_record"));
        assert!(prompt.contains("update_record"));
        assert!(prompt.contains("suggest"));
        assert!(prompt.contains("respond"));
        assert!(prompt.contains("run_command"));
    }

    #[test]
    fn system_prompt_contains_budget() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains(&MAX_ASSISTANT_ROUNDS.to_string()));
    }

    #[test]
    fn system_prompt_encourages_concise() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("concise"));
    }

    #[test]
    fn system_prompt_excludes_protected_tables() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(!prompt.contains("\"sessions\""));
        assert!(!prompt.contains("\"command_history\""));
        assert!(!prompt.contains("\"chat_messages\""));
    }

    #[test]
    fn system_prompt_includes_kb_data() {
        let conn = test_conn();
        {
            let c = conn.lock().unwrap();
            c.execute(
                "INSERT INTO hosts (session_id, ip, status) VALUES ('s1', '10.10.10.1', 'up')",
                [],
            ).unwrap();
        }
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("=== Hosts ==="));
        assert!(prompt.contains("10.10.10.1"));
    }

    #[test]
    fn max_assistant_rounds_is_twenty() {
        assert_eq!(MAX_ASSISTANT_ROUNDS, 20);
    }

    #[test]
    fn assistant_tools_are_six() {
        let conn = test_conn();
        let ctx = Arc::new(ToolContext {
            conn,
            session_id: "s1".into(),
            cwd: PathBuf::from("/tmp"),
        });
        let tools = vec![
            make_query_tool(ctx.clone()),
            make_create_tool(ctx.clone()),
            make_update_tool(ctx.clone()),
            make_run_command_tool(ctx),
            make_suggest_tool(),
            make_respond_tool(),
        ];
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"query_table"));
        assert!(names.contains(&"create_record"));
        assert!(names.contains(&"update_record"));
        assert!(names.contains(&"run_command"));
        assert!(names.contains(&"suggest"));
        assert!(names.contains(&"respond"));
    }

    #[test]
    fn system_prompt_no_skill_mode() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, true).unwrap();
        assert!(prompt.contains("Redtrail"));
        assert!(!prompt.contains("Active skill:"));
    }

    #[test]
    fn run_command_tool_executes() {
        let conn = test_conn();
        let ctx = Arc::new(ToolContext {
            conn,
            session_id: "s1".into(),
            cwd: PathBuf::from("/tmp"),
        });
        let tool = make_run_command_tool(ctx);
        let input = serde_json::json!({"command": "echo hello"});
        let result = tool.execute.call(input).unwrap();
        assert!(result.contains("hello"));
        assert!(result.contains("exit code: 0"));
    }

    #[test]
    fn run_command_tool_schema_valid() {
        let schema = schemars::schema_for!(RunCommandInput);
        let s = serde_json::to_value(&schema).unwrap();
        assert!(s["properties"]["command"].is_object());
    }

    #[test]
    fn resolve_active_skill_name_no_skill_mode() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let name = resolve_active_skill_name(&c, "s1", Path::new("/tmp"), None, true);
        assert!(name.is_none());
    }

    #[test]
    fn resolve_active_skill_name_override() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let name = resolve_active_skill_name(&c, "s1", Path::new("/tmp"), Some("redtrail-report"), false);
        assert_eq!(name.unwrap(), "redtrail-report");
    }

    #[test]
    fn resolve_active_skill_name_auto_detect() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let name = resolve_active_skill_name(&c, "s1", Path::new("/tmp"), None, false);
        assert_eq!(name.unwrap(), "redtrail-recon");
    }

    #[test]
    fn tool_filtering_applies_skill_tools() {
        let allowed = Some(vec!["query_table".to_string(), "suggest".to_string()]);
        let conn = test_conn();
        let ctx = Arc::new(ToolContext {
            conn,
            session_id: "s1".into(),
            cwd: PathBuf::from("/tmp"),
        });
        let all_tools = vec![
            make_query_tool(ctx.clone()),
            make_create_tool(ctx.clone()),
            make_update_tool(ctx.clone()),
            make_run_command_tool(ctx),
            make_suggest_tool(),
            make_respond_tool(),
        ];
        let tools: Vec<_> = match allowed {
            Some(ref a) => all_tools
                .into_iter()
                .filter(|t| a.iter().any(|name| name == &t.name))
                .collect(),
            None => all_tools,
        };
        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"query_table"));
        assert!(names.contains(&"suggest"));
        assert!(!names.contains(&"run_command"));
    }

    #[test]
    fn tool_filtering_none_keeps_all() {
        let allowed: Option<Vec<String>> = None;
        let conn = test_conn();
        let ctx = Arc::new(ToolContext {
            conn,
            session_id: "s1".into(),
            cwd: PathBuf::from("/tmp"),
        });
        let all_tools = vec![
            make_query_tool(ctx.clone()),
            make_create_tool(ctx.clone()),
            make_update_tool(ctx.clone()),
            make_run_command_tool(ctx),
            make_suggest_tool(),
            make_respond_tool(),
        ];
        let tools: Vec<_> = match allowed {
            Some(ref a) => all_tools
                .into_iter()
                .filter(|t| a.iter().any(|name| name == &t.name))
                .collect(),
            None => all_tools,
        };
        assert_eq!(tools.len(), 6);
    }
}
