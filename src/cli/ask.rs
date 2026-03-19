use std::path::Path;
use rusqlite::Connection;
use serde_json::{json, Value};
use crate::config::Config;
use crate::db;
use crate::error::Error;
use crate::workspace;

const MAX_TOOL_ROUNDS: usize = 20;
const MAX_OUTPUT_CHARS: usize = 12000;

pub fn run(message: Option<&str>, keep_history: bool, clear: bool, model_override: Option<&str>) -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let db_path = workspace::db_path(&ws);
    let db_path_str = db_path.to_str().ok_or(Error::Config("invalid db path".into()))?;
    let conn = db::open_connection(db_path_str)?;
    let session_id = db::session::active_session_id(&conn)?;

    if clear {
        let deleted = db::chat::clear(&conn, &session_id)?;
        println!("cleared {deleted} messages");
        return Ok(());
    }

    let message = message.ok_or(Error::Config("no message provided".into()))?;
    let config = Config::resolved(&ws)?;
    let model = model_override.unwrap_or(&config.general.llm_model);
    let system = build_system_prompt(&conn, &session_id, &cwd)?;

    let mut messages: Vec<Value> = if keep_history {
        db::chat::load(&conn, &session_id)?
            .into_iter()
            .map(|(role, content)| json!({"role": role, "content": content}))
            .collect()
    } else {
        vec![]
    };

    messages.push(json!({"role": "user", "content": message}));

    let client = build_client()?;
    let tools = tool_definitions();

    let mut final_text = String::new();

    for _ in 0..MAX_TOOL_ROUNDS {
        let response = call_api(&client, model, &system, &messages, &tools)?;
        let stop_reason = response["stop_reason"].as_str().unwrap_or("end_turn");
        let content = response["content"].clone();

        messages.push(json!({"role": "assistant", "content": content}));

        let blocks = content.as_array().cloned().unwrap_or_default();

        for block in &blocks {
            if block["type"] == "text" {
                if let Some(text) = block["text"].as_str() {
                    print!("{text}");
                    final_text.push_str(text);
                }
            }
        }

        if stop_reason != "tool_use" {
            println!();
            break;
        }

        let mut tool_results = vec![];
        for block in &blocks {
            if block["type"] == "tool_use" {
                let tool_id = block["id"].as_str().unwrap_or("");
                let tool_name = block["name"].as_str().unwrap_or("");
                let input = &block["input"];

                let result = execute_tool(tool_name, input, &conn, &cwd);
                let (content, is_error) = match result {
                    Ok(output) => (output, false),
                    Err(e) => (format!("error: {e}"), true),
                };

                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_id,
                    "content": content,
                    "is_error": is_error,
                }));
            }
        }

        messages.push(json!({"role": "user", "content": tool_results}));
    }

    if keep_history {
        db::chat::save(&conn, &session_id, "user", message)?;
        if !final_text.is_empty() {
            db::chat::save(&conn, &session_id, "assistant", &final_text)?;
        }
    }

    Ok(())
}

fn build_client() -> Result<reqwest::blocking::Client, Error> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| Error::Config(e.to_string()))
}

fn call_api(
    client: &reqwest::blocking::Client,
    model: &str,
    system: &str,
    messages: &[Value],
    tools: &[Value],
) -> Result<Value, Error> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| Error::Config("ANTHROPIC_API_KEY not set".into()))?;

    let mut body = json!({
        "model": model,
        "max_tokens": 8192,
        "system": system,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools);
    }

    let resp = client.post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| Error::Config(format!("API request failed: {e}")))?;

    let response: Value = resp.json()
        .map_err(|e| Error::Config(format!("API response parse failed: {e}")))?;

    if let Some(err) = response.get("error") {
        return Err(Error::Config(format!("API error: {}", err)));
    }

    Ok(response)
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "run_command",
            "description": "Execute a shell command in the workspace directory. Use for running pentesting tools, checking files, network operations, etc. Output is captured and returned.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "sql_query",
            "description": "Execute a read-only SQL query against the redtrail SQLite database. Tables: sessions, hosts, ports, credentials, access_levels, flags, hypotheses, evidence, command_history, notes, chat_messages. All tables have session_id column.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "SQL SELECT query"
                    }
                },
                "required": ["query"]
            }
        }),
    ]
}

fn execute_tool(name: &str, input: &Value, conn: &Connection, cwd: &Path) -> Result<String, Error> {
    match name {
        "run_command" => {
            let command = input["command"].as_str()
                .ok_or(Error::Config("missing command".into()))?;
            eprintln!("[tool] $ {command}");
            execute_command(command, cwd)
        }
        "sql_query" => {
            let query = input["query"].as_str()
                .ok_or(Error::Config("missing query".into()))?;
            eprintln!("[tool] sql: {query}");
            super::sql::execute_readonly_to_string(conn, query)
        }
        _ => Err(Error::Config(format!("unknown tool: {name}"))),
    }
}

fn execute_command(command: &str, cwd: &Path) -> Result<String, Error> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() { result.push('\n'); }
        result.push_str(&stderr);
    }
    if !output.status.success() {
        result.push_str(&format!("\n(exit code: {})", output.status.code().unwrap_or(-1)));
    }

    if result.len() > MAX_OUTPUT_CHARS {
        let mut end = MAX_OUTPUT_CHARS;
        while end > 0 && !result.is_char_boundary(end) { end -= 1; }
        result.truncate(end);
        result.push_str("\n... (output truncated)");
    }

    Ok(result)
}

fn build_system_prompt(conn: &Connection, session_id: &str, cwd: &Path) -> Result<String, Error> {
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

    let mut p = String::with_capacity(4096);
    p.push_str("You are Redtrail, a pentesting advisor embedded in a workspace. You help the operator by analyzing data, suggesting next steps, running commands, and querying the knowledge base.\n\n");
    p.push_str("Be concise and direct. Use pentesting terminology. When suggesting commands, prefer the tools already aliased in the workspace.\n\n");
    p.push_str(&format!("Target: {target}\nScope: {scope}\nGoal: {goal}\nPhase: {phase}\nNoise budget: {noise:.2}\nCWD: {}\n\n", cwd.display()));

    if !hosts.is_empty() {
        p.push_str("=== Hosts ===\n");
        for h in &hosts {
            p.push_str(&format!("  {} {} {}\n",
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
            p.push_str(&format!("  {}:{}/{} {} {}\n",
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
            p.push_str(&format!("  {}:{} @ {} ({})\n",
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
            p.push_str(&format!("  {} ({})\n",
                f["value"].as_str().unwrap_or(""),
                f["source"].as_str().unwrap_or("-"),
            ));
        }
        p.push('\n');
    }

    if !access.is_empty() {
        p.push_str("=== Access ===\n");
        for a in &access {
            p.push_str(&format!("  {}@{} level={} method={}\n",
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
            p.push_str(&format!("  [{}] {} — {} (priority={}, conf={:.1})\n",
                h["id"], h["statement"].as_str().unwrap_or(""),
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
            let exit = h["exit_code"].as_i64().map(|c| c.to_string()).unwrap_or("-".to_string());
            p.push_str(&format!("  [exit={}] {}\n",
                exit, h["command"].as_str().unwrap_or(""),
            ));
        }
        p.push('\n');
    }

    p.push_str("You have two tools:\n- run_command: execute shell commands\n- sql_query: query the redtrail database (read-only)\n\nUse them when needed to answer questions or perform actions. Always explain what you're doing.");

    Ok(p)
}
