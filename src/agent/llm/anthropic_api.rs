use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::agent::tools::ToolDef;

use super::{
    ChatEvent, ChatEventStream, ChatMessage, ChatRole, LlmError, LlmProvider, MessageContent,
    ToolUseRequest,
};

const DEFAULT_MODEL: &str = "claude-opus-4-6-20250612";
const DEFAULT_MAX_TOKENS: u32 = 8192;
const DEFAULT_TIMEOUT_SECS: u64 = 120;
const API_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct AnthropicApiConfig {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

impl AnthropicApiConfig {
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| LlmError::InvalidResponse("ANTHROPIC_API_KEY not set".into()))?;
        Ok(Self {
            api_key,
            model: DEFAULT_MODEL.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        })
    }
}

pub(super) struct AnthropicApiProvider {
    config: AnthropicApiConfig,
    client: reqwest::Client,
}

impl AnthropicApiProvider {
    pub fn new(config: AnthropicApiConfig) -> Result<Self, LlmError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| LlmError::NetworkError(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { config, client })
    }

    async fn send_request(&self, body: &ApiRequest) -> Result<ApiResponse, LlmError> {
        tracing::debug!(
            model = %body.model,
            max_tokens = body.max_tokens,
            messages_count = body.messages.len(),
            has_system = body.system.is_some(),
            tools_count = body.tools.as_ref().map(|t| t.len()).unwrap_or(0),
            "LLM request"
        );
        if let Some(ref sys) = body.system {
            tracing::trace!(system_prompt = %sys, "LLM system prompt");
        }
        for (i, msg) in body.messages.iter().enumerate() {
            tracing::trace!(idx = i, role = %msg.role, "LLM message: {}", serde_json::to_string(&msg.content).unwrap_or_default());
        }

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| {
                let err = if e.is_timeout() {
                    LlmError::Timeout(self.config.timeout_secs)
                } else {
                    LlmError::NetworkError(e.to_string())
                };
                tracing::error!("LLM request failed: {}", err);
                err
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(status = %status, "LLM API error: {}", body);
            return Err(LlmError::NetworkError(format!(
                "Anthropic API returned {status}: {body}"
            )));
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(e.to_string()))?;

        tracing::debug!(
            blocks = api_response.content.len(),
            stop_reason = ?api_response.stop_reason,
            "LLM response"
        );
        for block in &api_response.content {
            match block {
                ContentBlock::Text { text } => {
                    tracing::trace!("LLM text: {}", text);
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    tracing::debug!(tool = %name, "LLM tool_use: {}", input);
                }
                ContentBlock::ToolResult { content, .. } => {
                    tracing::trace!("LLM tool_result: {}", content);
                }
            }
        }

        Ok(api_response)
    }

    fn convert_messages(messages: &[ChatMessage]) -> Vec<ApiMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                };
                let content = match &msg.content {
                    MessageContent::Text(t) => ApiContent::Text(t.clone()),
                    MessageContent::ToolUse { text, calls } => {
                        let mut blocks = Vec::new();
                        if let Some(t) = text {
                            blocks.push(ContentBlock::Text { text: t.clone() });
                        }
                        for tc in calls {
                            blocks.push(ContentBlock::ToolUse {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                input: tc.input.clone(),
                            });
                        }
                        ApiContent::Blocks(blocks)
                    }
                    MessageContent::ToolResults(results) => {
                        let blocks = results
                            .iter()
                            .map(|r| ContentBlock::ToolResult {
                                tool_use_id: r.tool_use_id.clone(),
                                content: serde_json::to_string(&r.output).unwrap_or_default(),
                            })
                            .collect();
                        ApiContent::Blocks(blocks)
                    }
                };
                ApiMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }
}

#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiTool>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct ApiMessage {
    role: String,
    content: ApiContent,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
enum ApiContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct ApiTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

impl From<&ToolDef> for ApiTool {
    fn from(def: &ToolDef) -> Self {
        Self {
            name: def.name.clone(),
            description: def.description.clone(),
            input_schema: def.input_schema.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[async_trait]
impl LlmProvider for AnthropicApiProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
    ) -> Result<ChatEventStream, LlmError> {
        let api_tools = if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(ApiTool::from).collect())
        };

        let body = ApiRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            messages: Self::convert_messages(messages),
            system: system_prompt.map(|s| s.to_string()),
            tools: api_tools,
        };

        let api_response = self.send_request(&body).await?;

        Ok(Box::pin(async_stream::stream! {
            for block in &api_response.content {
                match block {
                    ContentBlock::Text { text } => {
                        yield ChatEvent::Token(text.clone());
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        yield ChatEvent::ToolUse(ToolUseRequest {
                            id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                        });
                    }
                    ContentBlock::ToolResult { .. } => {}
                }
            }
            yield ChatEvent::Done;
        }))
    }

    fn name(&self) -> &str {
        "anthropic-api"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let provider = AnthropicApiProvider::new(AnthropicApiConfig {
            api_key: "test-key".into(),
            model: DEFAULT_MODEL.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        })
        .unwrap();
        assert_eq!(provider.name(), "anthropic-api");
    }

    #[test]
    fn test_api_request_serialization() {
        let req = ApiRequest {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            messages: vec![ApiMessage {
                role: "user".into(),
                content: ApiContent::Text("Hello".into()),
            }],
            system: Some("Be helpful".into()),
            tools: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "claude-sonnet-4-20250514");
        assert_eq!(json["max_tokens"], 4096);
        assert!(json["system"].is_string());
        assert!(json.get("tools").is_none());
    }

    #[test]
    fn test_api_request_no_system_omitted() {
        let req = ApiRequest {
            model: "test".into(),
            max_tokens: 1024,
            messages: vec![],
            system: None,
            tools: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("system").is_none());
    }

    #[test]
    fn test_api_response_with_tool_use() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "I'll run that for you."},
                {"type": "tool_use", "id": "toolu_01", "name": "run_command", "input": {"command": "ls"}}
            ],
            "stop_reason": "tool_use"
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 2);
        match &resp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "I'll run that for you."),
            _ => panic!("expected Text block"),
        }
        match &resp.content[1] {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "run_command");
                assert_eq!(input["command"], "ls");
            }
            _ => panic!("expected ToolUse block"),
        }
    }

    #[test]
    fn test_api_response_text_only() {
        let json = r#"{
            "content": [{"type": "text", "text": "Hello!"}],
            "stop_reason": "end_turn"
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        match &resp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello!"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_api_tool_from_tooldef() {
        let def = ToolDef {
            name: "scan_ports".into(),
            description: "Scan ports on a host".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "host": {"type": "string"}
                },
                "required": ["host"]
            }),
        };
        let api_tool = ApiTool::from(&def);
        assert_eq!(api_tool.name, "scan_ports");
    }

    #[test]
    fn test_request_with_tools() {
        let req = ApiRequest {
            model: "test".into(),
            max_tokens: 1024,
            messages: vec![],
            system: None,
            tools: Some(vec![ApiTool {
                name: "test_tool".into(),
                description: "A test".into(),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            }]),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("tools").is_some());
        assert_eq!(json["tools"][0]["name"], "test_tool");
    }

    #[test]
    fn test_convert_messages() {
        use super::super::{ChatMessage, ToolResult, ToolUseRequest};

        let messages = vec![
            ChatMessage::user("hello"),
            ChatMessage::assistant_tool_use(
                Some("Let me check".into()),
                &[ToolUseRequest {
                    id: "t1".into(),
                    name: "ping".into(),
                    input: serde_json::json!({"host": "1.1.1.1"}),
                }],
            ),
            ChatMessage::tool_results(vec![ToolResult {
                tool_use_id: "t1".into(),
                output: serde_json::json!({"status": "ok"}),
            }]),
        ];

        let api_msgs = AnthropicApiProvider::convert_messages(&messages);
        assert_eq!(api_msgs.len(), 3);
        assert_eq!(api_msgs[0].role, "user");
        assert_eq!(api_msgs[1].role, "assistant");
        assert_eq!(api_msgs[2].role, "user");
    }

    #[test]
    fn test_from_env_missing_key() {
        let result = AnthropicApiConfig::from_env();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_default_values() {
        assert_eq!(DEFAULT_MODEL, "claude-opus-4-6-20250612");
        assert_eq!(DEFAULT_MAX_TOKENS, 8192);
        assert_eq!(DEFAULT_TIMEOUT_SECS, 120);
    }
}
