pub mod context;
pub mod prompt;
pub mod render;
pub mod stream;
pub mod tools;

pub use context::gather_context;
pub use prompt::{build_messages, build_system_prompt};

use crate::agent::llm::ChatMessage;
use crate::agent::tools::ToolRegistry;
use crate::error::Error;
use std::sync::Arc;

pub struct ChatInput {
    pub user_message: String,
    pub history: Vec<ChatMessage>,
}

pub struct ChatResult {
    pub block: crate::workflows::types::Block,
    pub updated_history: Vec<ChatMessage>,
    pub response_text: String,
}

pub struct ChatWorkflow {
    pub provider: Arc<dyn crate::agent::llm::LlmProvider>,
    pub tools: Arc<ToolRegistry>,
}

impl ChatWorkflow {
    pub async fn execute(&self, input: ChatInput) -> Result<ChatResult, Error> {
        stream::run_chat(self, input).await
    }
}
