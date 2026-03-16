use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::agent::tools::ToolDef;

use super::{
    ChatEvent, ChatEventStream, ChatMessage, ChatRole, LlmError, LlmProvider, MessageContent,
    ToolUseRequest,
};

const DEFAULT_MODEL: &str = "llama3-groq-tool-use:8b";
const DEFAULT_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OllamaConfig {
    pub model: String,
    pub base_url: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            base_url: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct OllamaProvider {
    client: reqwest::Client,
    config: OllamaConfig,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self, LlmError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| LlmError::NetworkError(format!("failed to build HTTP client: {e}")))?;
        Ok(Self { client, config })
    }

    fn convert_messages(messages: &[ChatMessage]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .flat_map(|msg| {
                let role = match msg.role {
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                };
                match &msg.content {
                    MessageContent::Text(t) => vec![OllamaMessage {
                        role: role.to_string(),
                        content: t.clone(),
                        tool_calls: None,
                        tool_name: None,
                    }],
                    MessageContent::ToolUse { text, calls } => vec![OllamaMessage {
                        role: "assistant".to_string(),
                        content: text.clone().unwrap_or_default(),
                        tool_calls: Some(
                            calls
                                .iter()
                                .map(|tc| OllamaToolCall {
                                    function: OllamaFunctionCall {
                                        name: tc.name.clone(),
                                        arguments: tc.input.clone(),
                                    },
                                })
                                .collect(),
                        ),
                        tool_name: None,
                    }],
                    MessageContent::ToolResults(results) => results
                        .iter()
                        .map(|r| OllamaMessage {
                            role: "tool".to_string(),
                            content: serde_json::to_string(&r.output).unwrap_or_default(),
                            tool_calls: None,
                            tool_name: None,
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn convert_tools(tools: &[ToolDef]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                r#type: "function".to_string(),
                function: OllamaFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect()
    }
}

// --- Ollama API types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    r#type: String,
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatChunk {
    message: Option<OllamaMessage>,
    done: bool,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
    ) -> Result<ChatEventStream, LlmError> {
        let ollama_tools = if tools.is_empty() {
            None
        } else {
            Some(Self::convert_tools(tools))
        };

        let body = OllamaChatRequest {
            model: self.config.model.clone(),
            messages: Self::convert_messages(messages),
            stream: true,
            tools: ollama_tools,
            system: system_prompt.map(|s| s.to_string()),
        };

        let url = format!("{}/api/chat", self.config.base_url);

        let res = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(300)
                } else {
                    LlmError::NetworkError(e.to_string())
                }
            })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(LlmError::NetworkError(format!(
                "ollama returned {status}: {body}"
            )));
        }

        let byte_stream = res.bytes_stream();

        Ok(Box::pin(async_stream::stream! {
            let mut byte_stream = std::pin::pin!(byte_stream);
            let mut tool_idx: usize = 0;

            while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        yield ChatEvent::Error(e.to_string());
                        return;
                    }
                };

                let parsed: OllamaChatChunk = match serde_json::from_slice(&bytes) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                if let Some(msg) = &parsed.message {
                    if !msg.content.is_empty() {
                        yield ChatEvent::Token(msg.content.clone());
                    }
                    if let Some(calls) = &msg.tool_calls {
                        for tc in calls {
                            yield ChatEvent::ToolUse(ToolUseRequest {
                                id: format!("{}_{tool_idx}", tc.function.name),
                                name: tc.function.name.clone(),
                                input: tc.function.arguments.clone(),
                            });
                            tool_idx += 1;
                        }
                    }
                }

                if parsed.done {
                    break;
                }
            }

            yield ChatEvent::Done;
        }))
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_tools() {
        let tools = vec![ToolDef {
            name: "ping".into(),
            description: "Ping a host".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "host": {"type": "string"}
                },
                "required": ["host"]
            }),
        }];

        let ollama_tools = OllamaProvider::convert_tools(&tools);
        assert_eq!(ollama_tools.len(), 1);
        assert_eq!(ollama_tools[0].r#type, "function");
        assert_eq!(ollama_tools[0].function.name, "ping");

        let json = serde_json::to_value(&ollama_tools[0]).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "ping");
        assert!(json["function"]["parameters"]["properties"]["host"].is_object());
    }

    #[test]
    fn test_convert_messages_text() {
        let messages = vec![ChatMessage::user("hello")];
        let ollama_msgs = OllamaProvider::convert_messages(&messages);
        assert_eq!(ollama_msgs.len(), 1);
        assert_eq!(ollama_msgs[0].role, "user");
        assert_eq!(ollama_msgs[0].content, "hello");
        assert!(ollama_msgs[0].tool_calls.is_none());
    }

    #[test]
    fn test_convert_messages_tool_use() {
        let messages = vec![ChatMessage::assistant_tool_use(
            Some("checking".into()),
            &[ToolUseRequest {
                id: "ping_0".into(),
                name: "ping".into(),
                input: serde_json::json!({"host": "1.1.1.1"}),
            }],
        )];
        let ollama_msgs = OllamaProvider::convert_messages(&messages);
        assert_eq!(ollama_msgs[0].role, "assistant");
        assert_eq!(ollama_msgs[0].content, "checking");
        let tc = ollama_msgs[0].tool_calls.as_ref().unwrap();
        assert_eq!(tc[0].function.name, "ping");
    }

    #[test]
    fn test_convert_messages_tool_results() {
        use super::super::ToolResult;
        let messages = vec![ChatMessage::tool_results(vec![ToolResult {
            tool_use_id: "ping".into(),
            output: serde_json::json!({"status": "ok"}),
        }])];
        let ollama_msgs = OllamaProvider::convert_messages(&messages);
        assert_eq!(ollama_msgs[0].role, "tool");
        assert!(ollama_msgs[0].content.contains("ok"));
    }

    #[test]
    fn test_parse_response_text() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true
        }"#;
        let resp: OllamaChatChunk = serde_json::from_str(json).unwrap();
        let msg = resp.message.unwrap();
        assert_eq!(msg.content, "Hello!");
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_parse_response_tool_call() {
        let json = r#"{
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "get_weather",
                        "arguments": {"city": "Tokyo"}
                    }
                }]
            },
            "done": true
        }"#;
        let resp: OllamaChatChunk = serde_json::from_str(json).unwrap();
        let tc = resp.message.unwrap().tool_calls.unwrap();
        assert_eq!(tc[0].function.name, "get_weather");
        assert_eq!(tc[0].function.arguments["city"], "Tokyo");
    }

    #[test]
    fn test_chat_request_serialization() {
        let req = OllamaChatRequest {
            model: "llama3.2".into(),
            messages: vec![OllamaMessage {
                role: "user".into(),
                content: "hi".into(),
                tool_calls: None,
                tool_name: None,
            }],
            stream: false,
            tools: Some(vec![OllamaTool {
                r#type: "function".into(),
                function: OllamaFunction {
                    name: "test".into(),
                    description: "A test tool".into(),
                    parameters: serde_json::json!({"type": "object", "properties": {}}),
                },
            }]),
            system: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "llama3.2");
        assert_eq!(json["stream"], false);
        assert_eq!(json["tools"][0]["type"], "function");
        assert_eq!(json["tools"][0]["function"]["name"], "test");
    }

    #[test]
    fn test_chat_request_no_tools_omitted() {
        let req = OllamaChatRequest {
            model: "test".into(),
            messages: vec![],
            stream: false,
            tools: None,
            system: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("tools").is_none());
    }
}
