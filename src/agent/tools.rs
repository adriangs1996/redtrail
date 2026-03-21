use aisdk::core::tools::{Tool, ToolExecute};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::dispatcher;
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

#[derive(Deserialize, JsonSchema)]
pub struct RunCommandInput {
    pub command: String,
}

pub fn make_run_command_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "run_command".into(),
        description: "Execute a shell command in the workspace directory. Use for running pentesting tools, checking files, network operations. Output is captured and returned.".into(),
        input_schema: schema_for!(RunCommandInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: RunCommandInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&input.command)
                .current_dir(&ctx.cwd)
                .output()
                .map_err(|e| format!("exec: {e}"))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut result = String::new();
            if !stdout.is_empty() {
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&stderr);
            }
            if !output.status.success() {
                result.push_str(&format!(
                    "\n(exit code: {})",
                    output.status.code().unwrap_or(-1)
                ));
            }

            if result.len() > MAX_OUTPUT_CHARS {
                let mut end = MAX_OUTPUT_CHARS;
                while end > 0 && !result.is_char_boundary(end) {
                    end -= 1;
                }
                result.truncate(end);
                result.push_str("\n... (output truncated)");
            }

            Ok(result)
        })),
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
