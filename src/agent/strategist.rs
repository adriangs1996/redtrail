use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::db;
use crate::db::schema;
use crate::error::Error;
use super::{Agent, ToolContext};
use super::tools::{make_query_tool, make_create_tool, make_update_tool, make_suggest_tool};
use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::LanguageModel;

pub const MAX_STRATEGIST_ROUNDS: usize = 10;

pub struct StrategistInput {
    pub new_records: Vec<serde_json::Value>,
}

impl StrategistInput {
    pub fn to_prompt(&self) -> String {
        serde_json::json!({
            "task": "advise",
            "trigger": "new_records",
            "new_records": self.new_records
        }).to_string()
    }
}

pub struct AdviseInput {
    pub question: String,
}

impl AdviseInput {
    pub fn to_prompt(&self) -> String {
        serde_json::json!({
            "task": "advise",
            "trigger": "user_request",
            "question": self.question
        }).to_string()
    }
}

pub fn build_system_prompt(conn: &Connection, session_id: &str, cwd: &Path) -> Result<String, Error> {
    let db_schema = schema::as_json(conn);
    let schema_str = serde_json::to_string_pretty(&db_schema).unwrap_or_default();

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

    let target = session["target"].as_str().unwrap_or("(none)");
    let scope = session["scope"].as_str().unwrap_or("(unrestricted)");
    let goal = session["goal"].as_str().unwrap_or("general");
    let phase = summary["phase"].as_str().unwrap_or("L0");
    let noise = summary["noise_budget"].as_f64().unwrap_or(1.0);

    let mut p = String::with_capacity(16384);

    p.push_str("You are Redtrail's strategic advisor — a pentesting strategist advising a human operator.\n\
        You provide full strategic analysis using deductive reasoning, hypothesis management, and attack path suggestions.\n\n");

    p.push_str(&format!("Target: {target}\nScope: {scope}\nGoal: {goal}\nPhase: {phase}\n"));
    p.push_str(&format!("Noise budget: {noise:.2}\nCWD: {}\n\n", cwd.display()));

    // L0-L4 Deductive Protocol
    p.push_str("## Deductive Methodology (L0-L4)\n\n\
        Apply layered deductive reasoning to guide analysis:\n\n\
        **L0 — Reconnaissance / Surface Mapping**\n\
        Enumerate the attack surface: hosts, ports, services, versions. Build the initial model.\n\
        Goal: complete picture of what's exposed. No hypotheses yet — just data gathering.\n\
        Advance when: surface is sufficiently mapped (key ports/services identified).\n\n\
        **L1 — Hypothesis Generation**\n\
        From L0 data, generate testable hypotheses about potential vulnerabilities and attack paths.\n\
        Each hypothesis must be: specific, falsifiable, and linked to observed evidence.\n\
        Example: \"SSH on port 22 accepts password auth → brute force may succeed with common creds.\"\n\
        Advance when: hypotheses cover major attack vectors.\n\n\
        **L2 — Probing / Hypothesis Testing**\n\
        Design targeted probes to confirm or refute each hypothesis. Minimize noise.\n\
        Track: which hypothesis is being tested, expected outcome, actual outcome.\n\
        Update hypothesis status: proposed → probing → confirmed/refuted.\n\
        Advance when: high-priority hypotheses resolved.\n\n\
        **L3 — Exploitation / Confirmation**\n\
        Confirmed hypotheses become exploitation targets. Plan exploitation with:\n\
        - Exact commands and expected output\n\
        - Fallback if primary exploit fails\n\
        - Impact assessment (what access does this grant?)\n\
        Advance when: access gained or exploitation exhausted.\n\n\
        **L4 — Post-Exploitation / Lateral Movement**\n\
        From gained access, enumerate internal surface. Repeat L0-L3 from new vantage point.\n\
        Track: access levels, pivots, credential reuse, privilege escalation paths.\n\n");

    // BISCL Framework
    p.push_str("## BISCL Hypothesis Categories\n\n\
        Categorize hypotheses using BISCL to ensure coverage:\n\
        - **B**anner/Version: version-specific CVEs, known vulns for identified software\n\
        - **I**nput Validation: injection points (SQLi, XSS, command injection, SSTI, path traversal)\n\
        - **S**ession/Auth: weak auth, session fixation, credential reuse, default creds, brute force\n\
        - **C**onfiguration: misconfigs, exposed admin panels, debug endpoints, directory listing\n\
        - **L**ogic: business logic flaws, IDOR, race conditions, privilege escalation\n\n");

    // Hypothesis Management
    p.push_str("## Hypothesis Management\n\n\
        Use create_record and update_record on the `hypotheses` table to track reasoning:\n\
        - Create hypotheses with: statement, category (BISCL), priority, confidence (0.0-1.0), status\n\
        - Valid statuses: proposed, probing, confirmed, refuted, exploited\n\
        - Valid priorities: low, medium, high, critical\n\
        - Update status as evidence arrives. Refute with evidence, don't just abandon.\n\
        - Link evidence to hypotheses using the `evidence` table (set hypothesis_id).\n\n");

    // Session Context
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
                "  [{}] {} — {} (cat={}, pri={}, conf={:.1})\n",
                h["id"],
                h["statement"].as_str().unwrap_or(""),
                h["status"].as_str().unwrap_or(""),
                h["category"].as_str().unwrap_or("-"),
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

    p.push_str("## Database Schema\n");
    p.push_str(&schema_str);
    p.push_str("\n\n");

    p.push_str("## Instructions\n\
        - When triggered by new_records: analyze what was discovered, update hypotheses, suggest next steps.\n\
        - When triggered by user_request: perform full strategic analysis answering the question.\n\
        - Use query_table to examine the full KB state and understand context.\n\
        - Use create_record/update_record to manage hypotheses (create new ones, update status, adjust confidence).\n\
        - Use the suggest tool for actionable next steps. Include exact commands with specific flags.\n\
        - Prioritize: critical=immediate exploitation, high=promising vectors, medium=further enum, low=background.\n\
        - One suggest call per distinct action. Be specific about tool flags and expected output.\n\
        - For ports, web_paths, and vulns: use `ip` instead of `host_id`.\n\
        - Always state which deductive layer (L0-L4) your analysis is operating at.\n\
        - Always categorize hypotheses using BISCL.\n\n");

    p.push_str(&format!(
        "## Budget\nYou have a maximum of {MAX_STRATEGIST_ROUNDS} tool-calling rounds. Be efficient — \
         query what you need, reason, create/update hypotheses, then suggest.\n"
    ));

    Ok(p)
}

pub fn build_strategist_agent<M: LanguageModel + TextInputSupport + ToolCallSupport>(
    model: M,
    conn: Arc<Mutex<Connection>>,
    session_id: String,
    cwd: PathBuf,
) -> Result<Agent<M>, Error> {
    let system = {
        let c = conn.lock().map_err(|e| Error::Config(format!("db lock: {e}")))?;
        build_system_prompt(&c, &session_id, &cwd)?
    };

    let ctx = Arc::new(ToolContext { conn, session_id, cwd });
    let tools = vec![
        make_query_tool(ctx.clone()),
        make_create_tool(ctx.clone()),
        make_update_tool(ctx),
        make_suggest_tool(),
    ];

    Ok(Agent::new(model, system, tools, MAX_STRATEGIST_ROUNDS))
}

pub fn collect_new_records(
    tool_calls: &[aisdk::core::tools::ToolCallInfo],
    tool_results: &[aisdk::core::tools::ToolResultInfo],
) -> Vec<serde_json::Value> {
    let mut records = Vec::new();
    for (call, result) in tool_calls.iter().zip(tool_results.iter()) {
        if call.tool.name != "create_record" {
            continue;
        }
        if let Ok(output) = &result.output
            && let Some(s) = output.as_str()
                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s)
                    && parsed["created"] == true {
                        records.push(serde_json::json!({
                            "table": call.input["table"],
                            "id": parsed["id"],
                        }));
                    }
    }
    records
}

pub fn collect_suggestions(
    tool_results: &[aisdk::core::tools::ToolResultInfo],
) -> Vec<serde_json::Value> {
    let mut suggestions = Vec::new();
    for result in tool_results {
        if result.tool.name != "suggest" {
            continue;
        }
        if let Ok(output) = &result.output
            && let Some(s) = output.as_str()
                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    suggestions.push(parsed);
                }
    }
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::agent::tools::{make_query_tool, make_create_tool, make_update_tool, make_suggest_tool};

    fn test_conn() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target) VALUES ('s1', 'test', '/tmp/test', '10.10.10.1')",
            [],
        ).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn test_ctx(conn: Arc<Mutex<Connection>>) -> Arc<ToolContext> {
        Arc::new(ToolContext {
            conn,
            session_id: "s1".into(),
            cwd: PathBuf::from("/tmp"),
        })
    }

    #[test]
    fn strategist_input_json_structure() {
        let input = StrategistInput {
            new_records: vec![
                serde_json::json!({"table": "hosts", "id": 1}),
                serde_json::json!({"table": "ports", "id": 2}),
            ],
        };
        let prompt = input.to_prompt();
        let v: serde_json::Value = serde_json::from_str(&prompt).unwrap();
        assert_eq!(v["task"], "advise");
        assert_eq!(v["trigger"], "new_records");
        assert_eq!(v["new_records"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn strategist_input_empty_records() {
        let input = StrategistInput {
            new_records: vec![],
        };
        let prompt = input.to_prompt();
        let v: serde_json::Value = serde_json::from_str(&prompt).unwrap();
        assert_eq!(v["new_records"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn advise_input_json_structure() {
        let input = AdviseInput {
            question: "What should I try next on port 22?".into(),
        };
        let prompt = input.to_prompt();
        let v: serde_json::Value = serde_json::from_str(&prompt).unwrap();
        assert_eq!(v["task"], "advise");
        assert_eq!(v["trigger"], "user_request");
        assert_eq!(v["question"], "What should I try next on port 22?");
    }

    #[test]
    fn advise_input_is_valid_json() {
        let input = AdviseInput {
            question: "analyze attack surface".into(),
        };
        let prompt = input.to_prompt();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&prompt);
        assert!(parsed.is_ok());
    }

    #[test]
    fn system_prompt_contains_schema() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("Database Schema"));
        assert!(prompt.contains("hosts"));
        assert!(prompt.contains("ports"));
    }

    #[test]
    fn system_prompt_contains_budget() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("Budget"));
        assert!(prompt.contains(&MAX_STRATEGIST_ROUNDS.to_string()));
    }

    #[test]
    fn system_prompt_is_advisor_framing() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("strategist"));
        assert!(prompt.contains("suggest"));
        assert!(prompt.contains("exact commands"));
    }

    #[test]
    fn system_prompt_excludes_protected_tables() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(!prompt.contains("\"sessions\""));
        assert!(!prompt.contains("\"command_history\""));
        assert!(!prompt.contains("\"chat_messages\""));
    }

    #[test]
    fn system_prompt_contains_l0_l4() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("L0 — Reconnaissance"));
        assert!(prompt.contains("L1 — Hypothesis Generation"));
        assert!(prompt.contains("L2 — Probing"));
        assert!(prompt.contains("L3 — Exploitation"));
        assert!(prompt.contains("L4 — Post-Exploitation"));
    }

    #[test]
    fn system_prompt_contains_biscl() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("BISCL"));
        assert!(prompt.contains("anner/Version"));
        assert!(prompt.contains("nput Validation"));
        assert!(prompt.contains("ession/Auth"));
        assert!(prompt.contains("onfiguration"));
        assert!(prompt.contains("ogic"));
    }

    #[test]
    fn system_prompt_contains_hypothesis_management() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("Hypothesis Management"));
        assert!(prompt.contains("create_record"));
        assert!(prompt.contains("update_record"));
        assert!(prompt.contains("hypotheses"));
        assert!(prompt.contains("proposed"));
        assert!(prompt.contains("confirmed"));
        assert!(prompt.contains("refuted"));
    }

    #[test]
    fn system_prompt_contains_session_context() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("Target: 10.10.10.1"));
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
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("=== Hosts ==="));
        assert!(prompt.contains("10.10.10.1"));
    }

    #[test]
    fn strategist_tools_are_exactly_four() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let tools = vec![
            make_query_tool(ctx.clone()),
            make_create_tool(ctx.clone()),
            make_update_tool(ctx),
            make_suggest_tool(),
        ];
        assert_eq!(tools.len(), 4);
        assert_eq!(tools[0].name, "query_table");
        assert_eq!(tools[1].name, "create_record");
        assert_eq!(tools[2].name, "update_record");
        assert_eq!(tools[3].name, "suggest");
    }

    #[test]
    fn max_strategist_rounds_is_ten() {
        assert_eq!(MAX_STRATEGIST_ROUNDS, 10);
    }

    #[test]
    fn collect_new_records_finds_created() {
        use aisdk::core::tools::{ToolCallInfo, ToolResultInfo, ToolDetails};

        let calls = vec![
            ToolCallInfo {
                tool: ToolDetails { name: "create_record".into(), ..Default::default() },
                input: serde_json::json!({"table": "hosts", "data": {"ip": "10.10.10.1"}}),
                extensions: Default::default(),
            },
            ToolCallInfo {
                tool: ToolDetails { name: "query_table".into(), ..Default::default() },
                input: serde_json::json!({"table": "ports"}),
                extensions: Default::default(),
            },
            ToolCallInfo {
                tool: ToolDetails { name: "create_record".into(), ..Default::default() },
                input: serde_json::json!({"table": "ports", "data": {"port": 22}}),
                extensions: Default::default(),
            },
        ];

        let mut r1 = ToolResultInfo::new("create_record");
        r1.output(serde_json::Value::String(r#"{"id":1,"created":true}"#.into()));
        let mut r2 = ToolResultInfo::new("query_table");
        r2.output(serde_json::Value::String("[]".into()));
        let mut r3 = ToolResultInfo::new("create_record");
        r3.output(serde_json::Value::String(r#"{"id":5,"created":false}"#.into()));

        let results = vec![r1, r2, r3];
        let new = collect_new_records(&calls, &results);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0]["table"], "hosts");
        assert_eq!(new[0]["id"], 1);
    }

    #[test]
    fn collect_new_records_empty_when_no_creates() {
        let calls: Vec<aisdk::core::tools::ToolCallInfo> = vec![];
        let results: Vec<aisdk::core::tools::ToolResultInfo> = vec![];
        let new = collect_new_records(&calls, &results);
        assert!(new.is_empty());
    }

    #[test]
    fn collect_suggestions_extracts_suggest_results() {
        use aisdk::core::tools::ToolResultInfo;

        let mut r1 = ToolResultInfo::new("query_table");
        r1.output(serde_json::Value::String("[]".into()));
        let mut r2 = ToolResultInfo::new("suggest");
        r2.output(serde_json::Value::String(
            r#"{"text":"Try SSH brute force","priority":"high"}"#.into()
        ));
        let mut r3 = ToolResultInfo::new("suggest");
        r3.output(serde_json::Value::String(
            r#"{"text":"Enumerate web paths","priority":"medium"}"#.into()
        ));

        let results = vec![r1, r2, r3];
        let suggestions = collect_suggestions(&results);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0]["priority"], "high");
        assert_eq!(suggestions[1]["text"], "Enumerate web paths");
    }

    #[test]
    fn collect_suggestions_empty_when_none() {
        use aisdk::core::tools::ToolResultInfo;

        let mut r1 = ToolResultInfo::new("query_table");
        r1.output(serde_json::Value::String("[]".into()));
        let results = vec![r1];
        let suggestions = collect_suggestions(&results);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn strategist_input_prompt_is_valid_json() {
        let input = StrategistInput {
            new_records: vec![
                serde_json::json!({"table": "hosts", "id": 1}),
            ],
        };
        let prompt = input.to_prompt();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&prompt);
        assert!(parsed.is_ok());
    }

    #[test]
    fn system_prompt_includes_ip_resolution_hint() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c, "s1", Path::new("/tmp")).unwrap();
        assert!(prompt.contains("`ip` instead of `host_id`"));
    }
}
