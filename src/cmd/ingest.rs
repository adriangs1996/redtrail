use crate::core::capture;
use crate::core::db;
use crate::core::secrets::engine::redact_secrets;
use crate::error::Error;
use rusqlite::Connection;

/// Ingest a Claude Code hook payload from stdin into the database.
pub fn run(conn: &Connection) -> Result<(), Error> {
    let input = std::io::read_to_string(std::io::stdin())
        .map_err(|e| Error::Config(format!("failed to read stdin: {e}")))?;

    let payload: serde_json::Value = serde_json::from_str(&input)
        .map_err(|e| Error::Config(format!("invalid JSON on stdin: {e}")))?;

    let tool_name = payload["tool_name"]
        .as_str()
        .ok_or_else(|| Error::Config("missing tool_name in payload".into()))?
        .to_string();

    let tool_input = &payload["tool_input"];
    let tool_response = payload.get("tool_response");
    let error_msg = payload["error"].as_str();
    let is_failure = error_msg.is_some();

    let cwd = payload["cwd"].as_str().map(String::from);
    let agent_session_id = payload["session_id"].as_str().unwrap_or("unknown");

    // Find or create a RedTrail session for this agent session
    let session_id =
        db::find_or_create_agent_session(conn, agent_session_id, cwd.as_deref(), "claude_code")?;

    // Derive command_raw — human-readable summary
    let command_raw = derive_command_raw(&tool_name, tool_input);

    // Derive command_binary, subcommand, args, flags
    let (command_binary, command_subcommand, command_args, command_flags) =
        derive_parsed_fields(&tool_name, tool_input);

    // Derive stdout/stderr
    let max_bytes = capture::MAX_STDOUT_BYTES;
    let (stdout, stderr, stdout_truncated) =
        derive_output(&tool_name, tool_response, error_msg, max_bytes);

    // Derive exit_code
    let exit_code = derive_exit_code(&tool_name, tool_response, is_failure);

    // Git context
    let git = cwd.as_deref().map(capture::git_context);
    let git_repo = git.as_ref().and_then(|g| g.repo.clone());
    let git_branch = git.as_ref().and_then(|g| g.branch.clone());

    // Secret redaction on all fields
    let r_command_raw = redact_secrets(&command_raw);
    let r_stdout = stdout.as_deref().map(redact_secrets);
    let r_stderr = stderr.as_deref().map(redact_secrets);
    let tool_input_str = serde_json::to_string(tool_input).ok();
    let tool_response_str = tool_response.map(|v| serde_json::to_string(v).unwrap_or_default());
    let r_tool_input = tool_input_str.as_deref().map(redact_secrets);
    let r_tool_response = tool_response_str.as_deref().map(redact_secrets);

    let was_redacted = r_command_raw != command_raw
        || stdout
            .as_deref()
            .is_some_and(|s| r_stdout.as_deref() != Some(s))
        || stderr
            .as_deref()
            .is_some_and(|s| r_stderr.as_deref() != Some(s))
        || tool_input_str
            .as_deref()
            .is_some_and(|s| r_tool_input.as_deref() != Some(s))
        || tool_response_str
            .as_deref()
            .is_some_and(|s| r_tool_response.as_deref() != Some(s));

    // Truncate tool_response for storage
    let r_tool_response = r_tool_response.map(|s| {
        if s.len() > max_bytes {
            capture::truncate_output(&s, max_bytes)
        } else {
            s
        }
    });

    db::insert_agent_event(
        conn,
        &db::AgentEvent {
            session_id,
            command_raw: r_command_raw,
            command_binary,
            command_subcommand,
            command_args,
            command_flags,
            cwd,
            git_repo,
            git_branch,
            exit_code,
            stdout: r_stdout,
            stderr: r_stderr,
            stdout_truncated,
            stderr_truncated: false,
            source: "claude_code".into(),
            agent_session_id: Some(agent_session_id.to_string()),
            is_automated: true,
            redacted: was_redacted,
            tool_name: tool_name.clone(),
            tool_input: r_tool_input,
            tool_response: r_tool_response,
        },
    )?;

    Ok(())
}

fn derive_command_raw(tool_name: &str, tool_input: &serde_json::Value) -> String {
    match tool_name {
        "Bash" => tool_input["command"].as_str().unwrap_or("").to_string(),
        "Edit" => format!("Edit {}", tool_input["file_path"].as_str().unwrap_or("?")),
        "Write" => format!("Write {}", tool_input["file_path"].as_str().unwrap_or("?")),
        "Read" => format!("Read {}", tool_input["file_path"].as_str().unwrap_or("?")),
        "Glob" => format!("Glob {}", tool_input["pattern"].as_str().unwrap_or("?")),
        "Grep" => format!("Grep {}", tool_input["pattern"].as_str().unwrap_or("?")),
        "Agent" => format!(
            "Agent {}",
            tool_input["description"].as_str().unwrap_or("?")
        ),
        _ => {
            let summary = serde_json::to_string(tool_input).unwrap_or_default();
            let truncated = if summary.len() > 120 {
                format!("{}...", &summary[..117])
            } else {
                summary
            };
            format!("{tool_name} {truncated}")
        }
    }
}

fn derive_parsed_fields(
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if tool_name == "Bash"
        && let Some(cmd) = tool_input["command"].as_str()
    {
        let parsed = capture::parse_command(cmd);
        let binary = if parsed.binary.is_empty() {
            None
        } else {
            Some(parsed.binary)
        };
        let args = serde_json::to_string(&parsed.args).ok();
        let flags = serde_json::to_string(&parsed.flags).ok();
        return (binary, parsed.subcommand, args, flags);
    }
    // Non-Bash: tool_name as command_binary
    (Some(tool_name.to_string()), None, None, None)
}

fn derive_output(
    tool_name: &str,
    tool_response: Option<&serde_json::Value>,
    error_msg: Option<&str>,
    max_bytes: usize,
) -> (Option<String>, Option<String>, bool) {
    // Failure events: error goes into stderr
    if let Some(err) = error_msg {
        return (None, Some(err.to_string()), false);
    }

    let response = match tool_response {
        Some(r) => r,
        None => return (None, None, false),
    };

    if tool_name == "Bash" {
        let stdout = response["stdout"].as_str().map(String::from);
        let stderr = response["stderr"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from);
        let truncated = stdout.as_ref().is_some_and(|s| s.len() > max_bytes);
        let stdout = stdout.map(|s| {
            if s.len() > max_bytes {
                capture::truncate_output(&s, max_bytes)
            } else {
                s
            }
        });
        (stdout, stderr, truncated)
    } else {
        // Non-Bash: serialize response as stdout
        let serialized = serde_json::to_string_pretty(response).unwrap_or_default();
        let truncated = serialized.len() > max_bytes;
        let stdout = if truncated {
            capture::truncate_output(&serialized, max_bytes)
        } else {
            serialized
        };
        (Some(stdout), None, truncated)
    }
}

fn derive_exit_code(
    tool_name: &str,
    tool_response: Option<&serde_json::Value>,
    is_failure: bool,
) -> Option<i32> {
    if is_failure {
        return Some(1);
    }
    if tool_name == "Bash" {
        // Try to read exitCode from the response object. If the response exists
        // but doesn't contain exitCode (e.g. PostToolUse sends stdout as a string),
        // default to 0 — the hook only fires on success.
        tool_response.map(|r| r["exitCode"].as_i64().map(|c| c as i32).unwrap_or(0))
    } else {
        None
    }
}
