use crate::core::capture;
use crate::core::db;
use crate::core::secrets::engine::redact_secrets;
use crate::error::Error;
use rusqlite::Connection;

/// Ingest a Claude Code hook payload from stdin into the database.
pub fn run(conn: &Connection, event_type: &str) -> Result<(), Error> {
    run_from_reader(conn, event_type, std::io::stdin())
}

/// Testable entry point: reads JSON from any reader, dispatches by event type.
pub fn run_from_reader(
    conn: &Connection,
    event_type: &str,
    reader: impl std::io::Read,
) -> Result<(), Error> {
    let input = std::io::read_to_string(reader)
        .map_err(|e| Error::Config(format!("failed to read input: {e}")))?;

    let payload: serde_json::Value =
        serde_json::from_str(&input).map_err(|e| Error::Config(format!("invalid JSON: {e}")))?;

    match event_type {
        "PostToolUse" | "PostToolUseFailure" => ingest_tool_event(conn, &payload),
        "SubagentStart" | "SubagentStop" | "UserPromptSubmit" | "SessionStart" | "SessionEnd"
        | "Stop" | "InstructionsLoaded" | "ConfigChange" => {
            ingest_lifecycle_event(conn, event_type, &payload)
        }
        _ => Err(Error::Config(format!("unknown event type: {event_type}"))),
    }
}

/// Ingest a PostToolUse or PostToolUseFailure event (existing logic).
fn ingest_tool_event(conn: &Connection, payload: &serde_json::Value) -> Result<(), Error> {
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

    let session_id =
        db::find_or_create_agent_session(conn, agent_session_id, cwd.as_deref(), "claude_code")?;

    let command_raw = derive_command_raw(&tool_name, tool_input);

    let (command_binary, command_subcommand, command_args, command_flags) =
        derive_parsed_fields(&tool_name, tool_input);

    let max_bytes = capture::MAX_STDOUT_BYTES;
    let (stdout, stderr, stdout_truncated) =
        derive_output(&tool_name, tool_response, error_msg, max_bytes);

    let exit_code = derive_exit_code(&tool_name, tool_response, is_failure);

    let git = cwd.as_deref().map(capture::git_context);
    let git_repo = git.as_ref().and_then(|g| g.repo.clone());
    let git_branch = git.as_ref().and_then(|g| g.branch.clone());

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

/// Ingest a lifecycle event (SubagentStart, UserPromptSubmit, SessionStart, etc.).
fn ingest_lifecycle_event(
    conn: &Connection,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), Error> {
    let agent_session_id = payload["session_id"].as_str().unwrap_or("unknown");
    let cwd = payload["cwd"].as_str().map(String::from);

    let session_id =
        db::find_or_create_agent_session(conn, agent_session_id, cwd.as_deref(), "claude_code")?;

    let (command_raw, command_binary, stdout, tool_input_json) = match event_type {
        "SubagentStart" => {
            let agent_type = payload["agent_type"].as_str().unwrap_or("unknown");
            (
                format!("Agent {agent_type} started"),
                "Agent",
                None,
                serde_json::json!({
                    "agent_id": payload["agent_id"],
                    "agent_type": payload["agent_type"]
                }),
            )
        }
        "SubagentStop" => {
            let agent_type = payload["agent_type"].as_str().unwrap_or("unknown");
            let last_msg = payload["last_assistant_message"].as_str().map(|s| {
                if s.len() > capture::MAX_STDOUT_BYTES {
                    capture::truncate_output(s, capture::MAX_STDOUT_BYTES)
                } else {
                    s.to_string()
                }
            });
            (
                format!("Agent {agent_type} stopped"),
                "Agent",
                last_msg,
                serde_json::json!({
                    "agent_id": payload["agent_id"],
                    "agent_type": payload["agent_type"],
                    "transcript_path": payload["agent_transcript_path"]
                }),
            )
        }
        "UserPromptSubmit" => {
            let prompt = payload["prompt"].as_str().unwrap_or("");
            (
                prompt.to_string(),
                "UserPromptSubmit",
                None,
                serde_json::json!({ "prompt": payload["prompt"] }),
            )
        }
        "SessionStart" => {
            let source = payload["source"].as_str().unwrap_or("unknown");
            let model = payload["model"].as_str().unwrap_or("unknown");
            (
                format!("Session started ({source}) model={model}"),
                "SessionStart",
                None,
                serde_json::json!({ "source": source, "model": model }),
            )
        }
        "SessionEnd" => (
            "Session ended".to_string(),
            "SessionEnd",
            None,
            serde_json::json!({}),
        ),
        "Stop" => {
            let reason = payload["stop_reason"].as_str().unwrap_or("unknown");
            (
                format!("Claude stopped ({reason})"),
                "Stop",
                None,
                serde_json::json!({ "stop_reason": reason }),
            )
        }
        "InstructionsLoaded" => {
            let file_path = payload["file_path"].as_str().unwrap_or("?");
            let load_reason = payload["load_reason"].as_str().unwrap_or("unknown");
            (
                format!("Loaded {file_path} ({load_reason})"),
                "InstructionsLoaded",
                None,
                serde_json::json!({
                    "file_path": file_path,
                    "memory_type": payload["memory_type"],
                    "load_reason": load_reason
                }),
            )
        }
        "ConfigChange" => (
            "Config changed".to_string(),
            "ConfigChange",
            None,
            serde_json::json!({ "config_source": payload["config_source"] }),
        ),
        _ => {
            return Err(Error::Config(format!(
                "unknown lifecycle event: {event_type}"
            )));
        }
    };

    // Secret redaction
    let r_command_raw = redact_secrets(&command_raw);
    let r_stdout = stdout.as_deref().map(redact_secrets);
    let tool_input_str = serde_json::to_string(&tool_input_json).unwrap_or_default();
    let r_tool_input = redact_secrets(&tool_input_str);

    let was_redacted = r_command_raw != command_raw
        || stdout
            .as_deref()
            .is_some_and(|s| r_stdout.as_deref() != Some(s))
        || r_tool_input != tool_input_str;

    db::insert_agent_event(
        conn,
        &db::AgentEvent {
            session_id,
            command_raw: r_command_raw,
            command_binary: Some(command_binary.to_string()),
            command_subcommand: None,
            command_args: None,
            command_flags: None,
            cwd,
            git_repo: None,
            git_branch: None,
            exit_code: None,
            stdout: r_stdout,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            source: "claude_code".into(),
            agent_session_id: Some(agent_session_id.to_string()),
            is_automated: true,
            redacted: was_redacted,
            tool_name: event_type.to_string(),
            tool_input: Some(r_tool_input),
            tool_response: None,
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
        "Skill" => format!(
            "Skill {}",
            tool_input["skill"]
                .as_str()
                .or_else(|| tool_input["name"].as_str())
                .unwrap_or("?")
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
