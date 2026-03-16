use crate::workflows::types::{Block, BlockContent, BlockStatus, ShellOutputLine, ShellOutputStream};
use std::time::Instant;

pub fn create_chat_block(block_id: usize, user_message: &str) -> Block {
    Block {
        id: block_id,
        command: format!("ask {}", user_message),
        content: BlockContent::Markdown(Vec::new()),
        status: BlockStatus::Running,
        collapsed: false,
        started_at: Instant::now(),
        job_id: None,
    }
}

pub fn append_token(block: &mut Block, token: &str) {
    block.content.push_token(token);
}

pub fn append_tool_call(block: &mut Block, name: &str, input: &serde_json::Value) {
    block.content.push_line(ShellOutputLine {
        text: format!("[tool] {} {}", name, input),
        stream: ShellOutputStream::Stderr,
    });
}

pub fn append_tool_result(block: &mut Block, name: &str, output: &serde_json::Value) {
    let text = match output.as_str() {
        Some(s) => s.to_string(),
        None => serde_json::to_string_pretty(output).unwrap_or_default(),
    };
    block.content.push_line(ShellOutputLine {
        text: format!("[result] {} \u{2192} {}", name, text),
        stream: ShellOutputStream::Stderr,
    });
}

pub fn finalize_block(block: &mut Block) {
    block.status = BlockStatus::Success(0);
}
