use redtrail::workflows::chat::render::{append_token, create_chat_block, finalize_block};
use redtrail::workflows::types::BlockStatus;

#[test]
fn chat_block_creation() {
    let block = create_chat_block(1, "what should I try next?");
    assert_eq!(block.command, "ask what should I try next?");
    assert!(matches!(block.status, BlockStatus::Running));
    assert_eq!(block.content.line_count(), 0);
}

#[test]
fn append_token_merges_inline() {
    let mut block = create_chat_block(1, "test");
    append_token(&mut block, "Hello ");
    append_token(&mut block, "world");
    assert_eq!(block.content.line_count(), 1);
    assert_eq!(block.content.lines_ref()[0].text, "Hello world");
}

#[test]
fn finalize_sets_success() {
    let mut block = create_chat_block(1, "test");
    append_token(&mut block, "response");
    finalize_block(&mut block);
    assert!(matches!(block.status, BlockStatus::Success(0)));
}
