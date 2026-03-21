use aisdk::core::tools::{Tool, ToolExecute};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::dispatcher;
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
