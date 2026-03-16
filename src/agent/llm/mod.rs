mod anthropic_api;
mod config;
mod ollama;

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};

use crate::agent::tools::{ToolDef, ToolRegistry};

#[derive(Debug, Clone)]
pub struct ToolUseRequest {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub output: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum ChatResponse {
    Text(String),
    ToolUse {
        text: Option<String>,
        tool_calls: Vec<ToolUseRequest>,
    },
}

#[derive(Debug, Clone)]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    ToolUse {
        text: Option<String>,
        calls: Vec<ToolUseRequest>,
    },
    ToolResults(Vec<ToolResult>),
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: MessageContent,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: MessageContent::Text(text.into()),
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: MessageContent::Text(text.into()),
        }
    }

    pub fn assistant_tool_use(text: Option<String>, calls: &[ToolUseRequest]) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: MessageContent::ToolUse {
                text,
                calls: calls.to_vec(),
            },
        }
    }

    pub fn tool_results(results: Vec<ToolResult>) -> Self {
        Self {
            role: ChatRole::User,
            content: MessageContent::ToolResults(results),
        }
    }
}

// --- Provider-level stream events (single LLM call) ---

#[derive(Debug, Clone)]
pub enum ChatEvent {
    Token(String),
    ToolUse(ToolUseRequest),
    Error(String),
    Done,
}

pub type ChatEventStream = Pin<Box<dyn Stream<Item = ChatEvent> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
    ) -> Result<ChatEventStream, LlmError>;

    fn name(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("provider timed out after {0}s")]
    Timeout(u64),
    #[error("process exited with non-zero status: {0}")]
    ProcessFailed(String),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

pub use anthropic_api::AnthropicApiConfig;
pub use config::{LlmConfig, create_provider};
pub use ollama::OllamaConfig;

pub async fn collect_chat_response(mut stream: ChatEventStream) -> Result<ChatResponse, LlmError> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    while let Some(event) = stream.next().await {
        match event {
            ChatEvent::Token(t) => text.push_str(&t),
            ChatEvent::ToolUse(tc) => tool_calls.push(tc),
            ChatEvent::Error(e) => return Err(LlmError::InvalidResponse(e)),
            ChatEvent::Done => break,
        }
    }

    if tool_calls.is_empty() {
        Ok(ChatResponse::Text(text))
    } else {
        Ok(ChatResponse::ToolUse {
            text: if text.is_empty() { None } else { Some(text) },
            tool_calls,
        })
    }
}

// --- Streaming tool loop ---

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ToolCallStart {
        name: String,
        input: serde_json::Value,
    },
    ToolCallDone {
        name: String,
        output: serde_json::Value,
    },
    Error(String),
    Done,
}

const DEFAULT_MAX_TOOL_ROUNDS: usize = 10;

pub type ChatStream = Pin<Box<dyn Stream<Item = StreamEvent> + Send>>;

pub fn chat_stream(
    provider: Arc<dyn LlmProvider>,
    prompt: String,
    tools: Vec<ToolDef>,
    registry: Arc<ToolRegistry>,
) -> ChatStream {
    Box::pin(async_stream::stream! {
        let mut messages = vec![ChatMessage::user(prompt)];

        for _ in 0..DEFAULT_MAX_TOOL_ROUNDS {
            let event_stream = match provider.chat(&messages, &tools, None).await {
                Ok(s) => s,
                Err(e) => {
                    yield StreamEvent::Error(e.to_string());
                    return;
                }
            };

            let mut text_acc = String::new();
            let mut tool_calls: Vec<ToolUseRequest> = Vec::new();

            let mut event_stream = std::pin::pin!(event_stream);
            while let Some(event) = event_stream.next().await {
                match event {
                    ChatEvent::Token(t) => {
                        yield StreamEvent::Token(t.clone());
                        text_acc.push_str(&t);
                    }
                    ChatEvent::ToolUse(tc) => {
                        tool_calls.push(tc);
                    }
                    ChatEvent::Error(e) => {
                        yield StreamEvent::Error(e);
                        return;
                    }
                    ChatEvent::Done => break,
                }
            }

            if tool_calls.is_empty() {
                yield StreamEvent::Done;
                return;
            }

            let text = if text_acc.is_empty() { None } else { Some(text_acc) };
            messages.push(ChatMessage::assistant_tool_use(text, &tool_calls));

            let mut results = Vec::new();
            for tc in &tool_calls {
                yield StreamEvent::ToolCallStart {
                    name: tc.name.clone(),
                    input: tc.input.clone(),
                };

                let output = registry
                    .call(&tc.name, tc.input.clone())
                    .await
                    .unwrap_or_else(|e| serde_json::json!({"error": e}));

                yield StreamEvent::ToolCallDone {
                    name: tc.name.clone(),
                    output: output.clone(),
                };

                results.push(ToolResult {
                    tool_use_id: tc.id.clone(),
                    output,
                });
            }
            messages.push(ChatMessage::tool_results(results));
        }

        yield StreamEvent::Error("tool loop exceeded max rounds".into());
    })
}
