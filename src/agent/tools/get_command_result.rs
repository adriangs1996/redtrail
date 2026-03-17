use std::sync::{Arc, Mutex};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::db_v2::DbV2;
use crate::agent::tools::{RegisteredTool, ToolDef, ToolHandler};

#[derive(Deserialize, JsonSchema)]
pub struct GetCommandResultInput {
    /// The command_history_id to look up
    pub command_history_id: i64,
}

pub fn get_command_result(db: Arc<Mutex<DbV2>>) -> RegisteredTool {
    use schemars::schema_for;

    let schema = schema_for!(GetCommandResultInput);
    let mut input_schema = serde_json::to_value(schema).unwrap();
    if let Some(obj) = input_schema.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
    }
    crate::agent::tools::simplify_schema(&mut input_schema);

    let def = ToolDef {
        name: "get_command_result".to_string(),
        description: "Look up the output/result of a previously executed command by its command_history_id".to_string(),
        input_schema,
    };

    let handler: ToolHandler = Box::new(move |raw: serde_json::Value| {
        let db = db.clone();
        Box::pin(async move {
            let input: GetCommandResultInput =
                serde_json::from_value(raw).map_err(|e| format!("invalid input: {e}"))?;

            let db = db.lock().map_err(|e| format!("db lock: {e}"))?;
            match db.get_command_result(input.command_history_id) {
                Ok(Some(r)) => Ok(serde_json::json!({
                    "command": r.command,
                    "exit_code": r.exit_code,
                    "duration_ms": r.duration_ms,
                    "output": r.output_preview,
                    "started_at": r.started_at,
                })),
                Ok(None) => Ok(serde_json::json!({
                    "error": format!("no command result with id {}", input.command_history_id)
                })),
                Err(e) => Ok(serde_json::json!({"error": e.to_string()})),
            }
        })
    });

    RegisteredTool { def, handler }
}
