use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::db;
use crate::error::Error;
use crate::skill_loader;
use super::{Agent, ToolContext};
use super::tools::*;
use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::LanguageModel;

pub const MAX_ASSISTANT_ROUNDS: usize = 25;

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

    let briefing = crate::db::briefing::build_briefing(conn, session_id)?;
    p.push_str(&briefing);
    p.push('\n');

    p.push_str(crate::db::briefing::SCHEMA_REFERENCE);
    p.push_str("\n\n");

    p.push_str("## Tools\n\
        - query_table: query KB rows (use after running commands that change state)\n\
        - create_record: insert new records (ip resolves to host_id for ports/web_paths/vulns)\n\
        - update_record: update existing records by id\n\
        - suggest: suggest a next action to the operator (priority: low/medium/high/critical)\n\
        - respond: send a response message to the operator\n\
        - run_command: execute shell commands in the workspace\n\n");

    p.push_str("## Instructions\n\
        - KB state is pre-loaded above. Use query_table ONLY after running commands that may have changed state.\n\
        - Batch tool calls when possible — multiple create_record, suggest, or run_command in one response.\n\
        - Use respond for your final answer. Use suggest for actionable next steps.\n\
        - Be concise. Explain what you're doing.\n\n");

    p.push_str(&format!(
        "## Budget\nYou have a maximum of {MAX_ASSISTANT_ROUNDS} tool-calling rounds. Batch aggressively.\n"
    ));

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
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES ('s1', 'test', '/tmp/test', '10.10.10.1', '10.10.10.0/24', 'ctf')",
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
        assert!(prompt.contains("Writable Tables"));
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
        assert!(prompt.contains("25"));
    }

    #[test]
    fn system_prompt_encourages_concise() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("concise"));
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
        assert!(prompt.contains("10.10.10.1"));
    }

    #[test]
    fn max_assistant_rounds_is_twentyfive() {
        assert_eq!(MAX_ASSISTANT_ROUNDS, 25);
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

    #[test]
    fn system_prompt_contains_batch_instruction() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp"), None, false).unwrap();
        assert!(prompt.contains("Batch aggressively"));
    }
}
