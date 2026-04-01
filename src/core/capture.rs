const DEFAULT_BLACKLIST: &[&str] = &[
    "vim", "nvim", "nano", "vi", "ssh", "scp", "top", "htop", "btop", "less", "more", "man",
    "tmux", "screen", "claude", "codex", "cursor", "aider", "cline", "opencode",
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

// --- Command parsing ---

/// Commands that have subcommands (binary → first non-flag arg is a subcommand).
const SUBCOMMAND_BINARIES: &[&str] = &[
    "git",
    "docker",
    "docker-compose",
    "podman",
    "kubectl",
    "helm",
    "terraform",
    "pulumi",
    "tofu",
    "cargo",
    "go",
    "npm",
    "yarn",
    "pnpm",
    "pip",
    "systemctl",
    "journalctl",
];

pub struct ParsedCommand {
    pub binary: String,
    pub subcommand: Option<String>,
    pub args: Vec<String>,
    pub flags: std::collections::HashMap<String, serde_json::Value>,
}

pub fn parse_command(command_raw: &str) -> ParsedCommand {
    let words = shell_words::split(command_raw).unwrap_or_default();
    if words.is_empty() {
        return ParsedCommand {
            binary: String::new(),
            subcommand: None,
            args: Vec::new(),
            flags: std::collections::HashMap::new(),
        };
    }

    let binary = words[0].clone();
    let has_subcommand = SUBCOMMAND_BINARIES.contains(&binary.as_str());

    let mut subcommand = None;
    let mut args = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut found_subcommand = false;

    for word in words.iter().skip(1) {
        if word.starts_with("--") {
            // Long flag: --amend, --force
            if let Some((key, val)) = word.split_once('=') {
                flags.insert(key.to_string(), serde_json::json!(val));
            } else {
                flags.insert(word.clone(), serde_json::json!(true));
            }
        } else if word.starts_with('-') {
            // Short flag: -m, -t, -la — store as boolean
            // We can't reliably tell which short flags take values without
            // per-binary knowledge, so we don't consume the next word.
            flags.insert(word.clone(), serde_json::json!(true));
        } else if has_subcommand && !found_subcommand {
            subcommand = Some(word.clone());
            found_subcommand = true;
        } else {
            args.push(word.clone());
        }
    }

    ParsedCommand {
        binary,
        subcommand,
        args,
        flags,
    }
}

// --- Duration parsing ---

/// Parse a human-friendly duration string into seconds.
/// Supports: "30s", "5m", "1h", "7d", or plain seconds "3600".
pub fn parse_duration(s: &str) -> Result<i64, crate::error::Error> {
    let s = s.trim();
    if s.is_empty() {
        return Err(crate::error::Error::Config("empty duration".into()));
    }

    if let Ok(secs) = s.parse::<i64>() {
        return Ok(secs);
    }

    let (num_str, suffix) = s.split_at(s.len() - 1);
    let num: i64 = num_str
        .parse()
        .map_err(|_| crate::error::Error::Config(format!("invalid duration: {s}")))?;

    match suffix {
        "s" => Ok(num),
        "m" => Ok(num * 60),
        "h" => Ok(num * 3600),
        "d" => Ok(num * 86400),
        _ => Err(crate::error::Error::Config(format!(
            "unknown duration suffix '{suffix}' in '{s}'. Use s/m/h/d"
        ))),
    }
}

// --- Env snapshot ---

pub const SNAPSHOT_ENV_VARS: &[&str] = &[
    "PATH",
    "VIRTUAL_ENV",
    "CONDA_DEFAULT_ENV",
    "NODE_ENV",
    "AWS_PROFILE",
    "KUBECONFIG",
    "DOCKER_HOST",
    "GOPATH",
    "RUST_LOG",
];

pub fn env_snapshot(env: &std::collections::HashMap<String, String>) -> String {
    let mut map = serde_json::Map::new();
    for key in SNAPSHOT_ENV_VARS {
        if let Some(val) = env.get(*key) {
            map.insert(key.to_string(), serde_json::Value::String(val.clone()));
        }
    }
    serde_json::Value::Object(map).to_string()
}

// --- Git context ---

pub struct GitContext {
    pub repo: Option<String>,
    pub branch: Option<String>,
}

pub fn git_context(cwd: &str) -> GitContext {
    let repo = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let branch = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let b = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if b.is_empty() { None } else { Some(b) }
        });

    GitContext { repo, branch }
}
