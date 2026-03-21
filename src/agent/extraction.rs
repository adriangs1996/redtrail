use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

use crate::db::schema;
use super::{Agent, ToolContext};
use super::tools::{make_query_tool, make_create_tool, make_update_tool};
use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::LanguageModel;

pub const MAX_EXTRACTION_ROUNDS: usize = 5;

pub struct ExtractionInput {
    pub command: String,
    pub tool: Option<String>,
    pub output: String,
}

impl ExtractionInput {
    pub fn should_skip(&self) -> bool {
        self.output.trim().is_empty()
    }

    pub fn to_prompt(&self) -> String {
        let tool_str = self.tool.as_deref().unwrap_or("unknown");
        serde_json::json!({
            "task": "extract",
            "command": self.command,
            "tool": tool_str,
            "output": self.output
        }).to_string()
    }
}

pub fn build_system_prompt(conn: &Connection) -> String {
    let db_schema = schema::as_json(conn);
    let schema_str = serde_json::to_string_pretty(&db_schema).unwrap_or_default();

    format!(
        "You are an extraction agent for a penetration testing knowledge base.\n\
         Your job is to parse command output and store structured findings using the provided tools.\n\
         \n\
         ## Database Schema\n\
         {schema_str}\n\
         \n\
         ## Instructions\n\
         - Extract ALL relevant information from the command output: hosts, ports, services, \
         credentials, vulnerabilities, web paths, flags, notes.\n\
         - Use create_record to insert new findings. Use query_table first to check if a record \
         already exists before creating duplicates.\n\
         - Use update_record to enrich existing records with new details.\n\
         - For ports, web_paths, and vulns: use the `ip` field instead of `host_id` — the system \
         resolves it automatically.\n\
         - Be thorough but precise. Only extract information that is clearly present in the output.\n\
         - Do NOT hallucinate or infer data that isn't in the output.\n\
         \n\
         ## Budget\n\
         You have a maximum of {MAX_EXTRACTION_ROUNDS} tool-calling rounds. Be efficient — batch \
         related queries and minimize redundant lookups."
    )
}

pub fn build_extraction_agent<M: LanguageModel + TextInputSupport + ToolCallSupport>(
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
    ];

    Agent::new(model, system, tools, MAX_EXTRACTION_ROUNDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::agent::tools::{make_query_tool, make_create_tool, make_update_tool};

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
    fn extraction_input_skip_empty() {
        let input = ExtractionInput {
            command: "nmap".into(),
            tool: None,
            output: "".into(),
        };
        assert!(input.should_skip());
    }

    #[test]
    fn extraction_input_skip_whitespace() {
        let input = ExtractionInput {
            command: "nmap".into(),
            tool: None,
            output: "   \n  ".into(),
        };
        assert!(input.should_skip());
    }

    #[test]
    fn extraction_input_not_skipped() {
        let input = ExtractionInput {
            command: "nmap -sV 10.10.10.1".into(),
            tool: Some("nmap".into()),
            output: "22/tcp open ssh OpenSSH 8.9".into(),
        };
        assert!(!input.should_skip());
    }

    #[test]
    fn extraction_input_json_structure() {
        let input = ExtractionInput {
            command: "nmap -sV 10.10.10.1".into(),
            tool: Some("nmap".into()),
            output: "22/tcp open ssh".into(),
        };
        let prompt = input.to_prompt();
        let v: serde_json::Value = serde_json::from_str(&prompt).unwrap();
        assert_eq!(v["task"], "extract");
        assert_eq!(v["command"], "nmap -sV 10.10.10.1");
        assert_eq!(v["tool"], "nmap");
        assert_eq!(v["output"], "22/tcp open ssh");
    }

    #[test]
    fn extraction_input_null_tool() {
        let input = ExtractionInput {
            command: "whoami".into(),
            tool: None,
            output: "root".into(),
        };
        let prompt = input.to_prompt();
        let v: serde_json::Value = serde_json::from_str(&prompt).unwrap();
        assert_eq!(v["tool"], "unknown");
    }

    #[test]
    fn system_prompt_contains_schema() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("Database Schema"));
        assert!(prompt.contains("hosts"));
        assert!(prompt.contains("ports"));
        assert!(prompt.contains("vulns"));
        assert!(prompt.contains("credentials"));
    }

    #[test]
    fn system_prompt_contains_budget() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("Budget"));
        assert!(prompt.contains(&MAX_EXTRACTION_ROUNDS.to_string()));
    }

    #[test]
    fn system_prompt_contains_extraction_instructions() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("extraction agent"));
        assert!(prompt.contains("create_record"));
        assert!(prompt.contains("query_table"));
        assert!(prompt.contains("update_record"));
        assert!(prompt.contains("Do NOT hallucinate"));
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
    fn system_prompt_includes_ip_resolution_hint() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("`ip` field instead of `host_id`"));
    }

    #[test]
    fn tool_create_error_returns_string() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let tool = make_create_tool(ctx);
        let input = serde_json::json!({
            "table": "sessions",
            "data": {"name": "hack"}
        });
        let result = tool.execute.call(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(!err_str.is_empty());
    }

    #[test]
    fn tool_query_error_returns_string() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let tool = make_query_tool(ctx);
        let input = serde_json::json!({
            "table": "nonexistent_table",
            "filters": {}
        });
        let result = tool.execute.call(input);
        assert!(result.is_err());
    }

    #[test]
    fn tool_update_error_returns_string() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let tool = make_update_tool(ctx);
        let input = serde_json::json!({
            "table": "sessions",
            "id": 1,
            "data": {"name": "x"}
        });
        let result = tool.execute.call(input);
        assert!(result.is_err());
    }

    #[test]
    fn tool_create_then_query_roundtrip() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let create = make_create_tool(ctx.clone());
        let query = make_query_tool(ctx);

        let cr = create.execute.call(serde_json::json!({
            "table": "hosts",
            "data": {"ip": "10.10.10.5", "status": "up"}
        })).unwrap();
        let cr_val: serde_json::Value = serde_json::from_str(&cr).unwrap();
        assert_eq!(cr_val["created"], true);

        let qr = query.execute.call(serde_json::json!({
            "table": "hosts",
            "filters": {"ip": "10.10.10.5"}
        })).unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&qr).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["status"], "up");
    }

    #[test]
    fn tool_create_query_update_roundtrip() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let create = make_create_tool(ctx.clone());
        let update = make_update_tool(ctx.clone());
        let query = make_query_tool(ctx);

        let cr = create.execute.call(serde_json::json!({
            "table": "hosts",
            "data": {"ip": "10.10.10.5"}
        })).unwrap();
        let cr_val: serde_json::Value = serde_json::from_str(&cr).unwrap();
        let host_id = cr_val["id"].as_i64().unwrap();

        let ur = update.execute.call(serde_json::json!({
            "table": "hosts",
            "id": host_id,
            "data": {"hostname": "target.htb", "status": "up"}
        })).unwrap();
        let ur_val: serde_json::Value = serde_json::from_str(&ur).unwrap();
        assert_eq!(ur_val["updated"], true);

        let qr = query.execute.call(serde_json::json!({
            "table": "hosts",
            "filters": {"ip": "10.10.10.5"}
        })).unwrap();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&qr).unwrap();
        assert_eq!(rows[0]["hostname"], "target.htb");
        assert_eq!(rows[0]["status"], "up");
    }

    #[test]
    fn extraction_tools_are_exactly_three() {
        let conn = test_conn();
        let ctx = test_ctx(conn);
        let tools = vec![
            make_query_tool(ctx.clone()),
            make_create_tool(ctx.clone()),
            make_update_tool(ctx),
        ];
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].name, "query_table");
        assert_eq!(tools[1].name, "create_record");
        assert_eq!(tools[2].name, "update_record");
    }

    #[test]
    fn max_extraction_rounds_is_five() {
        assert_eq!(MAX_EXTRACTION_ROUNDS, 5);
    }

    #[test]
    fn extraction_input_skip_none_output_equivalent() {
        let input = ExtractionInput {
            command: "cat /etc/passwd".into(),
            tool: None,
            output: String::new(),
        };
        assert!(input.should_skip());
    }

    #[test]
    fn extraction_input_prompt_is_valid_json() {
        let input = ExtractionInput {
            command: "nmap -sV --script=vuln 10.10.10.1".into(),
            tool: Some("nmap".into()),
            output: "PORT   STATE SERVICE\n22/tcp open  ssh\n80/tcp open  http".into(),
        };
        let prompt = input.to_prompt();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&prompt);
        assert!(parsed.is_ok());
        let v = parsed.unwrap();
        assert_eq!(v["task"], "extract");
        assert!(v["output"].as_str().unwrap().contains("22/tcp"));
    }

    #[test]
    fn system_prompt_schema_has_constraint_info() {
        let conn = test_conn();
        let c = conn.lock().unwrap();
        let prompt = build_system_prompt(&c);
        assert!(prompt.contains("up") && prompt.contains("down"));
        assert!(prompt.contains("tcp") && prompt.contains("udp"));
    }
}
