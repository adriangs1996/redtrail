pub mod tools;

use aisdk::core::DynamicModel;
use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::request::LanguageModelRequest;
use aisdk::core::language_model::LanguageModel;
use aisdk::core::language_model::generate_text::GenerateTextResponse;
use aisdk::core::language_model::stream_text::StreamTextResponse;
use aisdk::core::tools::Tool;
use aisdk::providers::anthropic::Anthropic;
use crate::config::Config;
use crate::error::Error;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn create_model(config: &Config) -> Result<Anthropic<DynamicModel>, Error> {
    match config.general.llm_provider.as_str() {
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| Error::Config("ANTHROPIC_API_KEY not set".into()))?;
            Anthropic::<DynamicModel>::builder()
                .model_name(&config.general.llm_model)
                .api_key(api_key)
                .build()
                .map_err(|e| Error::Config(format!("anthropic provider: {e}")))
        }
        other => Err(Error::Config(format!("unsupported llm_provider: {other}"))),
    }
}

pub struct ToolContext {
    pub conn: Arc<Mutex<Connection>>,
    pub session_id: String,
    pub cwd: PathBuf,
}

pub struct Agent<M: LanguageModel + TextInputSupport + ToolCallSupport> {
    model: M,
    system: String,
    tools: Vec<Tool>,
    max_steps: usize,
}

impl<M: LanguageModel + TextInputSupport + ToolCallSupport> Agent<M> {
    pub fn new(model: M, system: String, tools: Vec<Tool>, max_steps: usize) -> Self {
        Self { model, system, tools, max_steps }
    }

    fn build_request(&self, prompt: &str) -> LanguageModelRequest<M> {
        let mut builder = LanguageModelRequest::builder()
            .model(self.model.clone())
            .system(&self.system)
            .prompt(prompt);

        for tool in &self.tools {
            builder = builder.with_tool(tool.clone());
        }

        let max_steps = self.max_steps;
        builder = builder.stop_when(move |opts| {
            opts.steps().len() >= max_steps
        });

        builder.build()
    }

    pub async fn run(&self, prompt: &str) -> aisdk::Result<GenerateTextResponse> {
        let mut request = self.build_request(prompt);
        request.generate_text().await
    }

    pub async fn stream(&self, prompt: &str) -> aisdk::Result<StreamTextResponse> {
        let mut request = self.build_request(prompt);
        request.stream_text().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::tools::*;
    use crate::db;
    use schemars::schema_for;

    fn test_ctx() -> Arc<ToolContext> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, target) VALUES ('s1', 'test', '10.10.10.1')",
            [],
        ).unwrap();
        Arc::new(ToolContext {
            conn: Arc::new(Mutex::new(conn)),
            session_id: "s1".into(),
            cwd: PathBuf::from("/tmp"),
        })
    }

    #[test]
    fn tool_context_accessible_from_closures() {
        let ctx = test_ctx();
        let ctx2 = ctx.clone();
        let f = move || {
            let conn = ctx2.conn.lock().unwrap();
            let _sid = &ctx2.session_id;
            conn.query_row("SELECT 1", [], |_| Ok(())).unwrap();
        };
        f();
        assert_eq!(ctx.session_id, "s1");
    }

    #[test]
    fn make_query_tool_returns_valid_tool() {
        let ctx = test_ctx();
        let tool = make_query_tool(ctx);
        assert_eq!(tool.name, "query_table");
        assert!(!tool.description.is_empty());
    }

    #[test]
    fn make_create_tool_returns_valid_tool() {
        let ctx = test_ctx();
        let tool = make_create_tool(ctx);
        assert_eq!(tool.name, "create_record");
        assert!(!tool.description.is_empty());
    }

    #[test]
    fn make_update_tool_returns_valid_tool() {
        let ctx = test_ctx();
        let tool = make_update_tool(ctx);
        assert_eq!(tool.name, "update_record");
        assert!(!tool.description.is_empty());
    }

    #[test]
    fn query_tool_executes_successfully() {
        let ctx = test_ctx();
        {
            let conn = ctx.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO hosts (session_id, ip, hostname) VALUES ('s1', '10.10.10.1', 'target')",
                [],
            ).unwrap();
        }
        let tool = make_query_tool(ctx);
        let input = serde_json::json!({
            "table": "hosts",
            "filters": {}
        });
        let result = tool.execute.call(input).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.as_array().unwrap().len() >= 1);
    }

    #[test]
    fn create_tool_executes_successfully() {
        let ctx = test_ctx();
        let tool = make_create_tool(ctx);
        let input = serde_json::json!({
            "table": "hosts",
            "data": {"ip": "10.10.10.1"}
        });
        let result = tool.execute.call(input).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["created"], true);
    }

    #[test]
    fn update_tool_executes_successfully() {
        let ctx = test_ctx();
        {
            let conn = ctx.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
                [],
            ).unwrap();
        }
        let tool = make_update_tool(ctx);
        let input = serde_json::json!({
            "table": "hosts",
            "id": 1,
            "data": {"hostname": "target.htb"}
        });
        let result = tool.execute.call(input).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["updated"], true);
    }

    #[test]
    fn create_tool_returns_error_on_bad_table() {
        let ctx = test_ctx();
        let tool = make_create_tool(ctx);
        let input = serde_json::json!({
            "table": "sessions",
            "data": {"name": "hack"}
        });
        let result = tool.execute.call(input);
        assert!(result.is_err());
    }

    #[test]
    fn query_tool_with_filters() {
        let ctx = test_ctx();
        {
            let conn = ctx.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO hosts (session_id, ip, status) VALUES ('s1', '10.10.10.1', 'up')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO hosts (session_id, ip, status) VALUES ('s1', '10.10.10.2', 'down')",
                [],
            ).unwrap();
        }
        let tool = make_query_tool(ctx);
        let input = serde_json::json!({
            "table": "hosts",
            "filters": {"status": "up"}
        });
        let result = tool.execute.call(input).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let rows = parsed.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ip"], "10.10.10.1");
    }

    #[test]
    fn create_model_from_default_config() {
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "test-key-123") };
        let config = crate::config::Config::default();
        let model = super::create_model(&config).unwrap();
        assert_eq!(model.settings.provider_name, "anthropic");
    }

    #[test]
    fn create_model_unsupported_provider() {
        let mut config = crate::config::Config::default();
        config.general.llm_provider = "unsupported".into();
        let result = super::create_model(&config);
        assert!(result.is_err());
    }

    #[test]
    fn create_model_custom_model_name() {
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "test-key-123") };
        let mut config = crate::config::Config::default();
        config.general.llm_model = "claude-opus-4-20250514".into();
        let _model = super::create_model(&config).unwrap();
    }

    #[test]
    fn input_schemas_are_valid_json_schema() {
        let query_schema = schema_for!(QueryInput);
        let create_schema = schema_for!(CreateInput);
        let update_schema = schema_for!(UpdateInput);

        let q = serde_json::to_value(&query_schema).unwrap();
        assert!(q["properties"]["table"].is_object());
        assert!(q["properties"]["filters"].is_object());

        let c = serde_json::to_value(&create_schema).unwrap();
        assert!(c["properties"]["table"].is_object());
        assert!(c["properties"]["data"].is_object());

        let u = serde_json::to_value(&update_schema).unwrap();
        assert!(u["properties"]["table"].is_object());
        assert!(u["properties"]["id"].is_object());
        assert!(u["properties"]["data"].is_object());
    }
}
