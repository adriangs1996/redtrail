// LLM-powered summarization for smart mode agent-context.
// Calls Ollama via the shared `call_ollama` function.
// All functions degrade gracefully — never returns errors.
use crate::config::LlmConfig;
use crate::core::db::CommandRow;
use crate::core::errors::ErrorFixSequence;
use crate::core::fmt::paths::to_relative;
use crate::extract::llm::call_ollama;

use super::filter;

const SESSION_SYSTEM_PROMPT: &str = "\
You are summarizing a terminal coding session for a developer \
who will read this summary at the start of their NEXT coding session. \
They need to quickly understand:

1. What was being worked on (the goal/task)
2. What was accomplished
3. What key decisions were made and why
4. What problems were encountered and how they were resolved
5. What is unfinished, broken, or needs attention next

Be specific. Name files, functions, and error messages. \
Do NOT list raw commands. Synthesize into a narrative. \
Keep it to 5-8 sentences per session. \
Do NOT include file read operations or search operations \
in the summary — focus on what was CHANGED or ATTEMPTED.";

const ERROR_FIX_SYSTEM_PROMPT: &str = "\
Summarize each error→fix as a practical rule. \
Not 'run command X' but 'when you see error Y, the cause is Z \
and the fix is W'. Only include non-obvious fixes. \
Skip trivial retries. Output as a markdown bullet list.";

const DECISIONS_PROMPT: &str = "\
Extract key technical decisions from these coding sessions. \
Focus on: technology choices, architecture decisions, design patterns chosen, \
trade-offs discussed. Output as a markdown bullet list. \
If no clear decisions were made, output 'None identified.'";

const ADDITIONAL_RULES: &str = "\
Additional rules:
- Be DIRECT. No hedging (\"suggests\", \"implies\", \"may require\"). 
  State what happened.
- Be SPECIFIC. Name files, functions, error messages, commands. 
  Not \"multiple builds were required\" but \"ran cargo install 4 times 
  while iterating on src/cmd/agent_context/llm.rs\"
- Be SELECTIVE. Only include decisions that were MADE or CHANGED in 
  these sessions. Do not describe permanent project facts that are 
  obvious from the codebase (language, build system, module layout).
- If errors occurred that may not be fully resolved, create a 
  \"Needs Attention\" section with the specific test command to verify.
- Maximum length: 400 words for the entire output. Shorter is better. 
  Every sentence must earn its place.
";

/// Summarize a single session via LLM. Returns fallback text on failure.
pub fn summarize_session(
    config: &LlmConfig,
    commands: &[CommandRow],
    project_root: &str,
) -> String {
    let meaningful = filter::filter_meaningful(commands);
    if meaningful.is_empty() {
        return "(No meaningful commands in session)".to_string();
    }

    let prompt = build_session_prompt(&meaningful, project_root);
    call_llm(config, &prompt).unwrap_or_else(|| "(LLM unavailable)".to_string())
}

/// Summarize error-fix sequences via LLM. Returns empty string on failure.
pub fn summarize_error_fixes(config: &LlmConfig, sequences: &[ErrorFixSequence]) -> String {
    if sequences.is_empty() {
        return String::new();
    }

    let mut prompt =
        format!("{ERROR_FIX_SYSTEM_PROMPT}\n\nError-fix pairs:\n\n{ADDITIONAL_RULES}\n\n");
    for seq in sequences {
        let fix = seq.resolution_command.as_deref().unwrap_or("(manual fix)");
        prompt.push_str(&format!(
            "- Error: `{}` — stderr: {}\n  Fix: `{}`\n",
            seq.failing_command, seq.stderr_snippet, fix,
        ));
    }

    call_llm(config, &prompt).unwrap_or_default()
}

/// Extract technical decisions from sessions via LLM. Returns empty string on failure.
pub fn summarize_decisions(
    config: &LlmConfig,
    commands: &[CommandRow],
    project_root: &str,
) -> String {
    let meaningful = filter::filter_meaningful(commands);
    if meaningful.is_empty() {
        return String::new();
    }

    let mut prompt = format!("{DECISIONS_PROMPT}\n\nSession commands:\n\n{ADDITIONAL_RULES}\n\n");
    for cmd in meaningful.iter().take(50) {
        append_command_summary(&mut prompt, cmd, project_root);
    }

    call_llm(config, &prompt).unwrap_or_default()
}

fn build_session_prompt(commands: &[&CommandRow], project_root: &str) -> String {
    let mut prompt =
        format!("{SESSION_SYSTEM_PROMPT}\n\nSession commands:\n\n{ADDITIONAL_RULES}\n\n");
    for cmd in commands.iter().take(50) {
        append_command_summary(&mut prompt, cmd, project_root);
    }
    prompt
}

fn append_command_summary(prompt: &mut String, cmd: &CommandRow, project_root: &str) {
    let tool = cmd.tool_name.as_deref().unwrap_or("shell");
    let raw = cmd.command_raw.replace(project_root, ".");
    let exit = cmd
        .exit_code
        .map(|c| format!(" (exit {c})"))
        .unwrap_or_default();

    prompt.push_str(&format!("[{tool}] {raw}{exit}\n"));

    // Include stderr snippet for failed commands
    if cmd.exit_code.is_some_and(|c| c != 0)
        && let Some(stderr) = &cmd.stderr
    {
        let snippet: String = stderr.lines().take(3).collect::<Vec<_>>().join("\n");
        if !snippet.is_empty() {
            let rel_snippet = snippet.replace(project_root, ".");
            let rel = to_relative(&rel_snippet, project_root);
            prompt.push_str(&format!("  stderr: {rel}\n"));
        }
    }
}

fn call_llm(config: &LlmConfig, prompt: &str) -> Option<String> {
    let truncated = if prompt.len() > config.max_input_chars {
        &prompt[..config.max_input_chars]
    } else {
        prompt
    };

    match call_ollama(
        &config.ollama.url,
        &config.ollama.model,
        truncated,
        config.timeout_seconds,
    ) {
        Ok(response) => {
            let trimmed = response.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(e) => {
            eprintln!("[redtrail] agent-context llm: {e}");
            None
        }
    }
}
