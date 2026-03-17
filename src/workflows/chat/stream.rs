use futures::StreamExt;
use tokio::sync::mpsc;

use crate::agent::llm::{ChatEvent, ChatMessage, ToolResult, ToolUseRequest};
use crate::agent::tools::ToolDef;
use crate::error::Error;
use crate::workflows::types::BlockStatus;

use super::render::{append_token, append_tool_call, append_tool_result, create_chat_block, finalize_block};
use super::{ChatInput, ChatResult, ChatWorkflow};

static NEXT_BLOCK_ID: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(10000);

const MAX_TOOL_ROUNDS: usize = 10;

pub async fn run_chat(workflow: &ChatWorkflow, input: ChatInput) -> Result<ChatResult, Error> {
    run_chat_streaming(workflow, input, None).await
}

pub async fn run_chat_streaming(
    workflow: &ChatWorkflow,
    input: ChatInput,
    token_tx: Option<mpsc::UnboundedSender<String>>,
) -> Result<ChatResult, Error> {
    let block_id = NEXT_BLOCK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut block = create_chat_block(block_id, &input.user_message);

    let tools: Vec<ToolDef> = workflow.tools.definitions().iter().map(|t| (*t).clone()).collect();
    let mut system_prompt_buf = String::from(
        "You are Redtrail, a pentesting advisor. Be concise and actionable."
    );
    if !input.recent_commands.is_empty() {
        system_prompt_buf.push_str("\n\nRecent commands executed in this session:");
        for rc in &input.recent_commands {
            if let Some(id) = rc.command_history_id {
                system_prompt_buf.push_str(&format!("\n- `{}` (command_history_id={}; use get_command_result to see output)", rc.command, id));
            } else {
                system_prompt_buf.push_str(&format!("\n- `{}`", rc.command));
            }
        }
    }
    let system_prompt: &str = &system_prompt_buf;

    let mut messages: Vec<ChatMessage> = input.history.clone();
    messages.push(ChatMessage::user(&input.user_message));

    let mut response_text = String::new();

    for _ in 0..MAX_TOOL_ROUNDS {
        let event_stream = workflow
            .provider
            .chat(&messages, &tools, Some(system_prompt))
            .await
            .map_err(Error::Llm)?;

        let mut text_acc = String::new();
        let mut tool_calls: Vec<ToolUseRequest> = Vec::new();

        let mut event_stream = std::pin::pin!(event_stream);
        while let Some(event) = event_stream.next().await {
            match event {
                ChatEvent::Token(t) => {
                    response_text.push_str(&t);
                    text_acc.push_str(&t);
                    append_token(&mut block, &t);
                    if let Some(tx) = &token_tx {
                        let _ = tx.send(t);
                    }
                }
                ChatEvent::ToolUse(tc) => {
                    tool_calls.push(tc);
                }
                ChatEvent::Error(e) => {
                    block.status = BlockStatus::Failed(1);
                    return Err(Error::Llm(crate::agent::llm::LlmError::InvalidResponse(e)));
                }
                ChatEvent::Done => break,
            }
        }

        if tool_calls.is_empty() {
            finalize_block(&mut block);
            break;
        }

        let text_opt = if text_acc.is_empty() { None } else { Some(text_acc) };
        messages.push(ChatMessage::assistant_tool_use(text_opt, &tool_calls));

        let mut results = Vec::new();
        for tc in &tool_calls {
            append_tool_call(&mut block, &tc.name, &tc.input);

            let output = workflow
                .tools
                .call(&tc.name, tc.input.clone())
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": e}));

            append_tool_result(&mut block, &tc.name, &output);

            results.push(ToolResult {
                tool_use_id: tc.id.clone(),
                output,
            });
        }
        messages.push(ChatMessage::tool_results(results));
    }

    let mut updated_history = input.history;
    updated_history.push(ChatMessage::user(&input.user_message));
    updated_history.push(ChatMessage::assistant_text(&response_text));

    Ok(ChatResult {
        block,
        updated_history,
        response_text,
    })
}
