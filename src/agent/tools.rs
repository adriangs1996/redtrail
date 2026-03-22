use aisdk::core::tools::{Tool, ToolExecute};
use regex::Regex;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::{commands, dispatcher};
#[allow(unused_imports)]
use super::ToolContext;

#[derive(Deserialize, JsonSchema)]
pub struct QueryInput {
    pub table: String,
    #[serde(default)]
    pub filters: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateInput {
    pub table: String,
    pub data: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateInput {
    pub table: String,
    pub id: i64,
    pub data: HashMap<String, serde_json::Value>,
}

pub fn make_query_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "query_table".into(),
        description: "Query rows from a knowledge base table. Returns JSON array of matching rows. Supports optional key-value filters with AND semantics.".into(),
        input_schema: schema_for!(QueryInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: QueryInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            eprintln!("\x1b[2m⟳ query: {}\x1b[0m", input.table);
            let conn = ctx.conn.lock()
                .map_err(|e| format!("db lock: {e}"))?;
            let rows = dispatcher::query(&conn, &ctx.session_id, &input.table, &input.filters)
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&rows)
                .map_err(|e| format!("serialize: {e}"))
        })),
    }
}

pub fn make_create_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "create_record".into(),
        description: "Create a record in a knowledge base table. Returns {id, created} where created=false means duplicate existed. Supports ip-to-host_id resolution for ports/web_paths/vulns.".into(),
        input_schema: schema_for!(CreateInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: CreateInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            eprintln!("\x1b[2m⟳ create: {}\x1b[0m", input.table);
            let conn = ctx.conn.lock()
                .map_err(|e| format!("db lock: {e}"))?;
            let result = dispatcher::create(&conn, &ctx.session_id, &input.table, &input.data)
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&serde_json::json!({
                "id": result.id,
                "created": result.created,
            })).map_err(|e| format!("serialize: {e}"))
        })),
    }
}

pub fn make_update_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "update_record".into(),
        description: "Update a record in a knowledge base table by id. Returns {updated: true/false}.".into(),
        input_schema: schema_for!(UpdateInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: UpdateInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            eprintln!("\x1b[2m⟳ update: {} id={}\x1b[0m", input.table, input.id);
            let conn = ctx.conn.lock()
                .map_err(|e| format!("db lock: {e}"))?;
            let result = dispatcher::update(&conn, &ctx.session_id, &input.table, input.id, &input.data)
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&serde_json::json!({
                "updated": result.updated,
            })).map_err(|e| format!("serialize: {e}"))
        })),
    }
}

const MAX_OUTPUT_CHARS: usize = 12000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize, JsonSchema)]
pub struct RunCommandInput {
    pub command: String,
}

pub fn sanitize_output(raw: &str) -> String {
    let ansi_re = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b\[[\?]?[0-9;]*[a-zA-Z]").unwrap();
    let cleaned = ansi_re.replace_all(raw, "");

    let lines: Vec<&str> = cleaned.lines().collect();
    let filtered: Vec<&str> = lines
        .into_iter()
        .filter(|line| {
            let l = line.trim();
            if l.is_empty() {
                return true;
            }
            let has_progress_indicators =
                (l.contains('%') || l.contains("ETA") || l.contains("eta"))
                    && (l.contains('[') || l.contains('|') || l.contains('#') || l.contains('='));
            if has_progress_indicators && l.contains('\r') {
                return false;
            }
            let cr_count = line.matches('\r').count();
            if cr_count > 1 {
                return false;
            }
            true
        })
        .collect();

    let joined = filtered.join("\n");

    let newline_re = Regex::new(r"\n{3,}").unwrap();
    let result = newline_re.replace_all(&joined, "\n\n");

    result.into_owned()
}

pub fn chunk_output(sanitized: &str) -> String {
    if sanitized.len() <= MAX_OUTPUT_CHARS {
        return sanitized.to_string();
    }

    let mut end = MAX_OUTPUT_CHARS;
    while end > 0 && !sanitized.is_char_boundary(end) {
        end -= 1;
    }
    if let Some(nl) = sanitized[..end].rfind('\n') {
        end = nl;
    }

    let mut result = sanitized[..end].to_string();
    let remaining = sanitized.len() - end;
    result.push_str(&format!(
        "\n... (output truncated, {remaining} chars remaining)"
    ));
    result
}

pub fn make_run_command_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "run_command".into(),
        description: "Execute a shell command in the workspace directory. Use for running pentesting tools, checking files, network operations. Output is captured and returned with exit code. Timeout: 300s.".into(),
        input_schema: schema_for!(RunCommandInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: RunCommandInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;

            eprint!("\x1b[2m⟳ run: {}\x1b[0m", input.command);
            use std::io::Write;
            std::io::stderr().flush().ok();

            let cmd_id = {
                let conn = ctx.conn.lock().map_err(|e| format!("db lock: {e}"))?;
                commands::insert(&conn, &ctx.session_id, &input.command, None)
                    .map_err(|e| format!("log insert: {e}"))?
            };

            let start = Instant::now();

            let child = std::process::Command::new("sh")
                .arg("-c")
                .arg(&input.command)
                .current_dir(&ctx.cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("exec: {e}"))?;

            let output = match wait_with_timeout(child, COMMAND_TIMEOUT) {
                Ok(o) => o,
                Err(e) => {
                    let elapsed = start.elapsed().as_millis() as i64;
                    if let Ok(conn) = ctx.conn.lock() {
                        let _ = commands::finish(&conn, cmd_id, -1, elapsed, &format!("timeout: {e}"));
                    }
                    return Err(format!("command timed out after {}s", COMMAND_TIMEOUT.as_secs()));
                }
            };

            eprint!("\r\x1b[2K");
            std::io::stderr().flush().ok();

            let elapsed = start.elapsed().as_millis() as i64;
            let exit_code = output.status.code().unwrap_or(-1);

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut raw = String::new();
            if !stdout.is_empty() {
                raw.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !raw.is_empty() {
                    raw.push('\n');
                }
                raw.push_str(&stderr);
            }

            let sanitized = sanitize_output(&raw);
            let chunked = chunk_output(&sanitized);

            {
                let conn = ctx.conn.lock().map_err(|e| format!("db lock: {e}"))?;
                commands::finish(&conn, cmd_id, exit_code, elapsed, &sanitized)
                    .map_err(|e| format!("log finish: {e}"))?;
            }

            Ok(format!("{chunked}\n(exit code: {exit_code})"))
        })),
    }
}

fn wait_with_timeout(
    child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    use std::thread;

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let child = child;
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => {
            let _ = handle.join();
            result.map_err(|e| format!("wait: {e}"))
        }
        Err(_) => {
            Err("timeout".into())
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct SuggestInput {
    pub text: String,
    pub priority: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RespondInput {
    pub text: String,
}

pub fn make_suggest_tool() -> Tool {
    Tool {
        name: "suggest".into(),
        description: "Suggest a next action or insight to the operator. Does not modify state. The text will be displayed to the user. Priority indicates urgency: low, medium, high, critical.".into(),
        input_schema: schema_for!(SuggestInput),
        execute: ToolExecute::new(Box::new(|params| {
            let input: SuggestInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            let valid = ["low", "medium", "high", "critical"];
            if !valid.contains(&input.priority.as_str()) {
                return Err(format!("invalid priority '{}': must be one of {:?}", input.priority, valid));
            }
            serde_json::to_string(&serde_json::json!({
                "text": input.text,
                "priority": input.priority,
            })).map_err(|e| format!("serialize: {e}"))
        })),
    }
}

pub fn make_respond_tool() -> Tool {
    Tool {
        name: "respond".into(),
        description: "Respond to the operator with a message. Does not modify state. The text will be displayed to the user.".into(),
        input_schema: schema_for!(RespondInput),
        execute: ToolExecute::new(Box::new(|params| {
            let input: RespondInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            serde_json::to_string(&serde_json::json!({
                "text": input.text,
            })).map_err(|e| format!("serialize: {e}"))
        })),
    }
}
