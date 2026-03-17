mod persist;

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::types::Target;
use crate::db_v2::DbV2;
use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub env: HashMap<String, String>,
    pub target: Target,
    pub tool_config: HashMap<String, serde_json::Value>,
    pub llm_provider: String,
    pub llm_model: String,
    pub working_dir: PathBuf,
    pub prompt_template: String,
}

impl SessionContext {
    pub fn new(name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: chrono::Utc::now().to_rfc3339(),
            env: HashMap::new(),
            target: Target {
                base_url: None,
                hosts: vec![],
                exec_mode: crate::types::ExecMode::Local,
                auth_token: None,
                scope: vec![],
            },
            tool_config: HashMap::new(),
            llm_provider: "anthropic-api".into(),
            llm_model: "claude-opus-4-6-20250612".into(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            prompt_template: "redtrail:{session} {status}$ ".into(),
        }
    }
}

pub struct SessionWorkflow;

impl SessionWorkflow {
    pub fn save(db: &DbV2, ctx: &SessionContext) -> Result<(), Error> {
        persist::save(db, ctx)
    }

    pub fn load(db: &DbV2, id: &str) -> Result<SessionContext, Error> {
        persist::load(db, id)
    }

    pub fn list(db: &DbV2) -> Result<Vec<SessionContext>, Error> {
        persist::list(db)
    }

    pub fn delete(db: &DbV2, id: &str) -> Result<(), Error> {
        persist::delete(db, id)
    }

    pub fn clone_session(db: &DbV2, src_id: &str, dest_name: &str) -> Result<SessionContext, Error> {
        persist::clone_session(db, src_id, dest_name)
    }

    pub fn load_by_name(db: &DbV2, name: &str) -> Result<SessionContext, Error> {
        persist::load_by_name(db, name)
    }
}
