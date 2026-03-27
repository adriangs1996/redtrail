const DEFAULT_BLACKLIST: &[&str] = &[
    "vim", "nvim", "nano", "vi",
    "ssh", "scp",
    "top", "htop", "btop",
    "less", "more", "man",
    "tmux", "screen",
];

pub fn default_blacklist() -> Vec<String> {
    DEFAULT_BLACKLIST.iter().map(|s| s.to_string()).collect()
}

pub fn is_blacklisted(binary: &str, blacklist: &[String]) -> bool {
    blacklist.iter().any(|b| b == binary)
}

pub fn extract_binary(command_raw: &str) -> &str {
    command_raw.split_whitespace().next().unwrap_or("")
}

/// Default max stdout size: 50KB
pub const MAX_STDOUT_BYTES: usize = 50 * 1024;

/// Truncate output to max_bytes, preserving the beginning.
pub fn truncate_output(output: &str, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output.to_string();
    }
    let marker = "\n[TRUNCATED]";
    let cut_at = max_bytes.saturating_sub(marker.len());
    // Find a safe UTF-8 boundary
    let mut end = cut_at;
    while end > 0 && !output.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = output[..end].to_string();
    truncated.push_str(marker);
    truncated
}

// --- Agent detection ---

/// Detect the source of a command based on environment variables and parent process.
/// Returns one of: "human", "claude_code", "cursor", "codex", "aider", "cline", "unknown_agent".
pub fn detect_source(
    env: &std::collections::HashMap<String, String>,
    parent_process: Option<&str>,
) -> &'static str {
    // Check env vars first (most reliable)
    if env.contains_key("CLAUDE_CODE") || env.contains_key("CLAUDE_CODE_SESSION") {
        return "claude_code";
    }
    if env.contains_key("CURSOR_SESSION_ID") || env.contains_key("CURSOR_TRACE_ID") {
        return "cursor";
    }
    if env.contains_key("CODEX_SESSION") || env.contains_key("CODEX_CLI") {
        return "codex";
    }
    if env.contains_key("AIDER_SESSION") {
        return "aider";
    }
    if env.contains_key("CLINE_SESSION") {
        return "cline";
    }

    // Fall back to parent process name
    if let Some(parent) = parent_process {
        let p = parent.to_lowercase();
        if p.contains("claude") {
            return "claude_code";
        }
        if p.contains("cursor") {
            return "cursor";
        }
        if p.contains("codex") {
            return "codex";
        }
        if p.contains("aider") {
            return "aider";
        }
        if p.contains("cline") {
            return "cline";
        }
    }

    "human"
}

pub fn is_automated(source: &str) -> bool {
    source != "human"
}
