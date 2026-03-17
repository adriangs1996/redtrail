pub mod get_command_result;
pub mod query_kb;
pub mod run_command;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub type ToolFuture = Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;
pub type ToolHandler = Box<dyn Fn(serde_json::Value) -> ToolFuture + Send + Sync>;

pub struct RegisteredTool {
    pub def: ToolDef,
    pub handler: ToolHandler,
}

pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: RegisteredTool) {
        self.tools.insert(tool.def.name.clone(), tool);
    }

    pub fn definitions(&self) -> Vec<&ToolDef> {
        self.tools.values().map(|t| &t.def).collect()
    }

    pub async fn call(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        (tool.handler)(input).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Simplify schemars-generated JSON Schema for LLM tool calling.
/// Converts `"type": ["string", "null"]` → `"type": "string"` and
/// strips `format`/`minimum` noise that confuses smaller models.
pub fn simplify_schema(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(obj) => {
            if let Some(serde_json::Value::Array(types)) = obj.get("type") {
                let non_null: Vec<_> = types
                    .iter()
                    .filter(|t| t.as_str() != Some("null"))
                    .cloned()
                    .collect();
                if non_null.len() == 1 {
                    obj.insert("type".into(), non_null[0].clone());
                }
            }
            obj.remove("format");
            obj.remove("minimum");
            for v in obj.values_mut() {
                simplify_schema(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                simplify_schema(v);
            }
        }
        _ => {}
    }
}

/// Generate a ToolDef + handler from a struct (must derive Deserialize + JsonSchema)
/// and an async handler function.
///
/// Usage:
/// ```ignore
/// #[derive(Deserialize, JsonSchema)]
/// struct RunCommandInput {
///     /// The command to execute
///     command: String,
///     /// Timeout in seconds
///     timeout: Option<u32>,
/// }
///
/// define_tool!(
///     run_command,
///     "Execute a shell command on the target",
///     RunCommandInput,
///     |input: RunCommandInput| async move {
///         Ok(serde_json::json!({ "output": "done" }))
///     }
/// );
///
/// // Then register:
/// registry.register(run_command());
/// ```
#[macro_export]
macro_rules! define_tool {
    ($name:ident, $desc:expr, $input_ty:ty, $handler:expr) => {
        pub fn $name() -> $crate::agent::tools::RegisteredTool {
            use schemars::schema_for;

            let schema = schema_for!($input_ty);
            let mut input_schema = serde_json::to_value(schema).unwrap();

            if let Some(obj) = input_schema.as_object_mut() {
                obj.remove("$schema");
                obj.remove("title");
            }
            $crate::agent::tools::simplify_schema(&mut input_schema);

            let def = $crate::agent::tools::ToolDef {
                name: stringify!($name).to_string(),
                description: $desc.to_string(),
                input_schema,
            };

            let handler: $crate::agent::tools::ToolHandler =
                Box::new(move |raw: serde_json::Value| {
                    Box::pin(async move {
                        let input: $input_ty = serde_json::from_value(raw)
                            .map_err(|e| format!("invalid input: {e}"))?;
                        let f = $handler;
                        f(input).await
                    })
                });

            $crate::agent::tools::RegisteredTool { def, handler }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;

    #[derive(Deserialize, JsonSchema)]
    struct PingInput {
        /// Target host to ping
        host: String,
        /// Number of packets
        count: Option<u32>,
    }

    define_tool!(
        ping_host,
        "Ping a target host",
        PingInput,
        |input: PingInput| async move {
            Ok(serde_json::json!({
                "host": input.host,
                "count": input.count.unwrap_or(4),
                "status": "ok"
            }))
        }
    );

    #[test]
    fn test_define_tool_generates_correct_schema() {
        let tool = ping_host();
        assert_eq!(tool.def.name, "ping_host");
        assert_eq!(tool.def.description, "Ping a target host");

        let schema = &tool.def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["host"].is_object());
        assert!(schema["properties"]["count"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("host")));
        assert!(!required.contains(&serde_json::json!("count")));

        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
    }

    #[tokio::test]
    async fn test_define_tool_handler_executes() {
        let tool = ping_host();
        let input = serde_json::json!({"host": "10.10.10.1", "count": 2});
        let result = (tool.handler)(input).await.unwrap();
        assert_eq!(result["host"], "10.10.10.1");
        assert_eq!(result["count"], 2);
        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn test_define_tool_handler_defaults() {
        let tool = ping_host();
        let input = serde_json::json!({"host": "10.10.10.1"});
        let result = (tool.handler)(input).await.unwrap();
        assert_eq!(result["count"], 4);
    }

    #[tokio::test]
    async fn test_define_tool_handler_invalid_input() {
        let tool = ping_host();
        let input = serde_json::json!({"wrong_field": true});
        let result = (tool.handler)(input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_register_and_call() {
        let mut registry = ToolRegistry::new();
        registry.register(ping_host());

        assert_eq!(registry.definitions().len(), 1);
        assert_eq!(registry.definitions()[0].name, "ping_host");

        let result = registry
            .call("ping_host", serde_json::json!({"host": "1.1.1.1"}))
            .await
            .unwrap();
        assert_eq!(result["host"], "1.1.1.1");
    }

    #[tokio::test]
    async fn test_registry_unknown_tool() {
        let registry = ToolRegistry::new();
        let result = registry.call("nope", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tool"));
    }
}
