use crate::agent::llm::ChatMessage;

const SYSTEM_PROMPT: &str = r#"You are Redtrail, a pentesting advisor embedded in an interactive shell.
You have access to tools for running commands and querying the knowledge base.
The user is a skilled penetration tester. Be concise and actionable.
When suggesting commands, prefix them with ! so the user can execute directly.
Focus on the current session context provided below."#;

pub fn build_system_prompt(context: &str) -> String {
    format!("{}\n\n## Current Context\n\n{}", SYSTEM_PROMPT, context)
}

pub fn build_messages(history: &[ChatMessage], user_input: &str) -> Vec<ChatMessage> {
    let mut messages = history.to_vec();
    messages.push(ChatMessage::user(user_input));
    messages
}
