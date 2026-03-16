use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::{
    agent::llm::{ChatEvent, ChatEventStream, ChatMessage, ToolResult, ToolUseRequest},
    agent::tools::ToolRegistry,
    backend::{Context, Handle},
    tui::DriverEvent,
};

const BASE_SYSTEM_PROMPT: &str = "\
You are Redtrail, a penetration testing assistant with access to tools.

CRITICAL RULES:
1. When you need to perform an action, you MUST use the tool calling mechanism provided by the API. \
DO NOT write JSON, code blocks, or any textual representation of a tool call. \
The system will invoke the tool for you automatically when you use the proper mechanism.
2. When you receive a tool result, read it carefully and use the data to answer the user directly.
3. If a tool returns an error, explain what went wrong.
4. Never say you cannot execute commands — you have a tool for that.
5. Never hallucinate tool results. Only report data you received from an actual tool call.
6. Use query_kb to inspect the knowledge base when you need context about what has been discovered so far.";

const MAX_TOOL_ROUNDS: usize = 10;

pub struct ProcessInput {
    input: String,
}

impl ProcessInput {
    pub fn new(input: String) -> Self {
        Self { input }
    }
}

struct StreamResult {
    text: String,
    tool_calls: Vec<ToolUseRequest>,
}

async fn consume_stream(
    stream: ChatEventStream,
    tx: &mpsc::Sender<DriverEvent>,
) -> Result<StreamResult, String> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut stream = std::pin::pin!(stream);

    while let Some(event) = stream.next().await {
        match event {
            ChatEvent::Token(t) => {
                let _ = tx.send(DriverEvent::Token(t.clone())).await;
                text.push_str(&t);
            }
            ChatEvent::ToolUse(tc) => tool_calls.push(tc),
            ChatEvent::Error(e) => return Err(e),
            ChatEvent::Done => break,
        }
    }

    Ok(StreamResult { text, tool_calls })
}

async fn execute_tools(
    calls: &[ToolUseRequest],
    registry: &ToolRegistry,
    tx: &mpsc::Sender<DriverEvent>,
) -> Vec<ToolResult> {
    let mut results = Vec::new();

    for tc in calls {
        let _ = tx
            .send(DriverEvent::Token(format!("\n[calling {}…]\n", tc.name)))
            .await;

        let output = registry
            .call(&tc.name, tc.input.clone())
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": e}));

        let _ = tx
            .send(DriverEvent::Token(format!("[{} done]\n", tc.name)))
            .await;

        results.push(ToolResult {
            tool_use_id: tc.id.clone(),
            output,
        });
    }

    results
}

#[async_trait]
impl Handle for ProcessInput {
    async fn handle(&self, ctx: &mut Context) {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            return;
        }

        let tools: Vec<_> = ctx.tools.definitions().into_iter().cloned().collect();
        let mut messages = vec![ChatMessage::user(trimmed)];

        let kb_summary = ctx.knowledge.read().await.to_context_summary();
        let system_prompt = if kb_summary.is_empty() {
            BASE_SYSTEM_PROMPT.to_string()
        } else {
            format!("{BASE_SYSTEM_PROMPT}\n{kb_summary}")
        };

        for _ in 0..MAX_TOOL_ROUNDS {
            let stream = match ctx
                .provider
                .chat(&messages, &tools, Some(&system_prompt))
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    let _ = ctx.event_tx.send(DriverEvent::Error(e.to_string())).await;
                    break;
                }
            };

            let result = match consume_stream(stream, &ctx.event_tx).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = ctx.event_tx.send(DriverEvent::Error(e)).await;
                    break;
                }
            };

            if result.tool_calls.is_empty() {
                break;
            }

            let text = if result.text.is_empty() {
                None
            } else {
                Some(result.text)
            };
            messages.push(ChatMessage::assistant_tool_use(text, &result.tool_calls));
            messages.push(ChatMessage::tool_results(
                execute_tools(&result.tool_calls, &ctx.tools, &ctx.event_tx).await,
            ));
        }

        let _ = ctx.event_tx.send(DriverEvent::Done).await;
    }
}
