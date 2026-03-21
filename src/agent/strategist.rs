use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use crate::db::schema;
use super::{Agent, ToolContext};
use super::tools::{make_query_tool, make_create_tool, make_update_tool, make_suggest_tool};
use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::LanguageModel;

pub const MAX_STRATEGIST_ROUNDS: usize = 5;

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

pub fn build_system_prompt(conn: &Connection) -> String {
    let db_schema = schema::as_json(conn);
    let schema_str = serde_json::to_string_pretty(&db_schema).unwrap_or_default();

    format!(
        "You are a pentesting strategist. After new data has been extracted from a command, \
         you analyze it and suggest next steps to the operator.\n\
         \n\
         ## Database Schema\n\
         {schema_str}\n\
         \n\
         ## Instructions\n\
         - You receive a list of newly created records from extraction.\n\
         - Use query_table to examine the full KB state and understand context.\n\
         - Analyze what was discovered and what it means for the engagement.\n\
         - Use the suggest tool to provide actionable next steps. Include exact commands \
         with specific flags and parameters.\n\
         - Prioritize suggestions: critical for immediate exploitation opportunities, \
         high for promising attack vectors, medium for further enumeration, low for background tasks.\n\
         - Keep suggestions concise and actionable. One suggest call per distinct action.\n\
         - You may use create_record or update_record to add hypotheses or notes based on your analysis.\n\
         - For ports, web_paths, and vulns: use the `ip` field instead of `host_id`.\n\
         \n\
         ## Budget\n\
         You have a maximum of {MAX_STRATEGIST_ROUNDS} tool-calling rounds. Be efficient — \
         query what you need, then suggest."
    )
}

pub fn build_strategist_agent<M: LanguageModel + TextInputSupport + ToolCallSupport>(
    model: M,
    conn: Arc<Mutex<Connection>>,
    session_id: String,
    cwd: PathBuf,
) -> Agent<M> {
    let system = {
        let c = conn.lock().unwrap();
        build_system_prompt(&c)
    };

    let ctx = Arc::new(ToolContext { conn, session_id, cwd });
    let tools = vec![
        make_query_tool(ctx.clone()),
        make_create_tool(ctx.clone()),
        make_update_tool(ctx),
        make_suggest_tool(),
    ];

    Agent::new(model, system, tools, MAX_STRATEGIST_ROUNDS)
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
        if let Ok(output) = &result.output {
            if let Some(s) = output.as_str() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    if parsed["created"] == true {
                        records.push(serde_json::json!({
                            "table": call.input["table"],
                            "id": parsed["id"],
                        }));
                    }
                }
            }
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
        if let Ok(output) = &result.output {
            if let Some(s) = output.as_str() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    suggestions.push(parsed);
                }
            }
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
            "INSERT INTO sessions (id, name, target) VALUES ('s1', 'test', '10.10.10.1')",
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
    fn system_prompt_contains_schema() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("Database Schema"));
        assert!(prompt.contains("hosts"));
        assert!(prompt.contains("ports"));
    }

    #[test]
    fn system_prompt_contains_budget() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("Budget"));
        assert!(prompt.contains(&MAX_STRATEGIST_ROUNDS.to_string()));
    }

    #[test]
    fn system_prompt_is_advisor_framing() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("strategist"));
        assert!(prompt.contains("suggest"));
        assert!(prompt.contains("exact commands"));
    }

    #[test]
    fn system_prompt_excludes_protected_tables() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(!prompt.contains("\"sessions\""));
        assert!(!prompt.contains("\"command_history\""));
        assert!(!prompt.contains("\"chat_messages\""));
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
    fn max_strategist_rounds_is_five() {
        assert_eq!(MAX_STRATEGIST_ROUNDS, 5);
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
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("`ip` field instead of `host_id`"));
    }
}
