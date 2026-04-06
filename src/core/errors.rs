/// Error normalization and error-fix sequence detection.
use std::sync::LazyLock;

use regex::Regex;

use crate::core::classify::{classify_command, is_project_command};
use crate::core::db::CommandRow;

static RE_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:/[a-zA-Z0-9_.@-]+){2,}(?:\.[a-zA-Z0-9]+)?").unwrap());

static RE_LINE_COL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":\d+:\d+:?").unwrap());

static RE_TIMESTAMP_ISO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}[Z\d:.+-]*").unwrap());

/// Normalize an error message for matching:
/// strips ANSI, file paths, line numbers, timestamps, lowercases, trims.
pub fn normalize_error(input: &str) -> String {
    let stripped = strip_ansi_escapes::strip_str(input);
    let s = RE_TIMESTAMP_ISO.replace_all(&stripped, "");
    let s = RE_PATH.replace_all(&s, "<path>");
    let s = RE_LINE_COL.replace_all(&s, ":<line>:");
    let s = s.to_lowercase();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the most relevant error line(s) from potentially long stderr output.
pub fn extract_error_lines(input: &str) -> String {
    let error_keywords = [
        "error:",
        "error[",
        "failed",
        "fatal:",
        "panic",
        "exception",
        "traceback",
        "abort",
        "cannot find",
        "not found",
        "undefined",
        "unresolved",
    ];
    let lines: Vec<&str> = input.lines().collect();
    let matched: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let lower = line.to_lowercase();
            error_keywords.iter().any(|kw| lower.contains(kw))
        })
        .copied()
        .collect();
    if !matched.is_empty() {
        return matched.into_iter().take(5).collect::<Vec<_>>().join("\n");
    }
    // Fallback: last 10 lines
    lines
        .into_iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone)]
pub struct ErrorFixSequence {
    pub failing_command: String,
    pub error_message: String,
    pub fix_actions: Vec<String>,
    pub resolution_command: Option<String>,
    pub resolved: bool,
    /// True when the resolution command has the same raw text as the failing command (a retry).
    pub is_retry: bool,
    /// Extracted error lines from stderr (via `extract_error_lines`).
    pub stderr_snippet: String,
}

/// Detect error-fix sequences in a list of commands.
/// Commands are sorted internally by timestamp_start ASC.
pub fn detect_error_fix_sequences(commands: &[CommandRow]) -> Vec<ErrorFixSequence> {
    let mut sorted: Vec<&CommandRow> = commands.iter().collect();
    sorted.sort_by_key(|c| c.timestamp_start);

    let mut sequences = Vec::new();
    let mut i = 0;

    while i < sorted.len() {
        let cmd = sorted[i];
        if cmd.exit_code.is_some_and(|c| c != 0) {
            let failing_binary = cmd.command_binary.as_deref().unwrap_or("");
            let error_msg = cmd
                .stderr
                .as_deref()
                .unwrap_or("(no error output)")
                .lines()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");

            let mut fix_actions = Vec::new();
            let mut resolution = None;
            let mut resolved = false;

            for next in &sorted[(i + 1)..] {
                if next.session_id != cmd.session_id {
                    break;
                }
                let next_binary = next.command_binary.as_deref().unwrap_or("");

                if next_binary == failing_binary && next.exit_code == Some(0) {
                    resolution = Some(next.command_raw.clone());
                    resolved = true;
                    break;
                }

                let category = classify_command(next_binary, None, None);
                if !category.is_read_only() {
                    fix_actions.push(next.command_raw.clone());
                }
            }

            // A retry is when the same command succeeds WITHOUT any intermediate
            // fix actions. If there were edits/writes between failure and success,
            // it's a genuine fix, not a retry.
            let is_retry = resolved
                && fix_actions.is_empty()
                && resolution
                    .as_ref()
                    .is_some_and(|r| r.trim() == cmd.command_raw.trim());

            let stderr_snippet = cmd
                .stderr
                .as_deref()
                .map(extract_error_lines)
                .unwrap_or_default();

            sequences.push(ErrorFixSequence {
                failing_command: cmd.command_raw.clone(),
                error_message: error_msg,
                fix_actions,
                resolution_command: resolution,
                resolved,
                is_retry,
                stderr_snippet,
            });
        }
        i += 1;
    }
    sequences
}

/// Like `detect_error_fix_sequences`, but filters the results:
/// - Retries (same command succeeding with no intermediate fix actions)
/// - Claude Code tool-level errors (tool_name set but not "Bash")
/// - Non-project commands (only keeps cargo, npm, python, etc.)
///
/// All commands are still passed to the detector so intermediate fix actions
/// (edits, writes) are properly tracked for retry vs. real-fix classification.
pub fn detect_error_fix_sequences_filtered(commands: &[CommandRow]) -> Vec<ErrorFixSequence> {
    detect_error_fix_sequences(commands)
        .into_iter()
        .filter(|seq| !seq.is_retry)
        .filter(|seq| {
            // Only keep errors from project commands executed via Bash (not tool errors)
            let parts: Vec<&str> = seq.failing_command.split_whitespace().collect();
            let binary = parts.first().copied().unwrap_or("");
            is_project_command(binary)
        })
        .collect()
}
