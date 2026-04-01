use crate::config::{Config, OnDetect};
use crate::core::capture;
use crate::core::db;
use crate::core::secrets::engine::{load_custom_patterns, redact_with_custom_patterns, CustomPattern};
use crate::error::Error;
use rusqlite::Connection;

pub struct StartArgs<'a> {
    pub session_id: &'a str,
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub config: &'a Config,
}

pub struct FinishArgs<'a> {
    pub command_id: &'a str,
    pub exit_code: Option<i32>,
    pub cwd: Option<&'a str>,
    pub config: &'a Config,
}

/// Create a running command record (called by preexec hook).
/// Prints the new command ID to stdout so the shell hook can capture it.
pub fn start(conn: &Connection, args: &StartArgs) -> Result<(), Error> {
    if !args.config.capture.enabled {
        return Ok(());
    }

    let parsed = capture::parse_command(args.command);

    if capture::is_blacklisted(&parsed.binary, &args.config.capture.blacklist_commands) {
        return Ok(());
    }

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let source = capture::detect_source(&env, None);

    let custom_patterns: Vec<CustomPattern> = args
        .config
        .secrets
        .patterns_file
        .as_deref()
        .map(load_custom_patterns)
        .unwrap_or_default();

    // Secret handling on command_raw
    let command_raw: std::borrow::Cow<str> = match args.config.secrets.on_detect {
        OnDetect::Block => {
            let (_, labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            if !labels.is_empty() {
                return Ok(());
            }
            std::borrow::Cow::Borrowed(args.command)
        }
        OnDetect::Redact => {
            let (redacted, _) = redact_with_custom_patterns(args.command, &custom_patterns);
            std::borrow::Cow::Owned(redacted)
        }
        OnDetect::Warn => {
            let (_, labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            if !labels.is_empty() {
                eprintln!(
                    "[redtrail] WARN: secrets detected ({}), storing unredacted per on_detect=warn",
                    labels.join(", ")
                );
            }
            std::borrow::Cow::Borrowed(args.command)
        }
    };

    let redacted = command_raw.as_ref() != args.command;

    let args_json = serde_json::to_string(&parsed.args).unwrap_or_default();
    let flags_json = serde_json::to_string(&parsed.flags).unwrap_or_default();

    let _ = db::cleanup_orphaned_commands(conn, args.session_id);

    let id = db::insert_command_start(
        conn,
        &db::NewCommandStart {
            session_id: args.session_id,
            command_raw: &command_raw,
            command_binary: if parsed.binary.is_empty() {
                None
            } else {
                Some(&parsed.binary)
            },
            command_subcommand: parsed.subcommand.as_deref(),
            command_args: Some(&args_json),
            command_flags: Some(&flags_json),
            cwd: args.cwd,
            shell: args.shell,
            hostname: args.hostname,
            source,
            redacted,
        },
    )?;

    print!("{id}");
    Ok(())
}

/// Finalize a running command record (called by precmd hook).
pub fn finish(conn: &Connection, args: &FinishArgs) -> Result<(), Error> {
    // Check if command row exists — tee may have deleted it in block mode
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM commands WHERE id = ?1",
            [args.command_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !exists {
        return Ok(());
    }

    let git = args.cwd.map(capture::git_context);
    let git_repo = git.as_ref().and_then(|g| g.repo.as_deref());
    let git_branch = git.as_ref().and_then(|g| g.branch.as_deref());

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let env_snap = capture::env_snapshot(&env);

    let custom_patterns: Vec<CustomPattern> = args
        .config
        .secrets
        .patterns_file
        .as_deref()
        .map(load_custom_patterns)
        .unwrap_or_default();

    // Read stdout/stderr from the row for final secret check
    let (stdout, stderr): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT stdout, stderr FROM commands WHERE id = ?1",
            [args.command_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    // Handle on_detect modes on final stdout/stderr
    match args.config.secrets.on_detect {
        OnDetect::Block => {
            let stdout_has_secrets = stdout
                .as_deref()
                .is_some_and(|s| !redact_with_custom_patterns(s, &custom_patterns).1.is_empty());
            let stderr_has_secrets = stderr
                .as_deref()
                .is_some_and(|s| !redact_with_custom_patterns(s, &custom_patterns).1.is_empty());

            if stdout_has_secrets || stderr_has_secrets {
                db::delete_command(conn, args.command_id)?;
                return Ok(());
            }
        }
        OnDetect::Redact => {
            // Re-redact full content (defense-in-depth)
            let redacted_stdout = stdout
                .as_deref()
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).0);
            let redacted_stderr = stderr
                .as_deref()
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).0);

            db::finish_command(
                conn,
                &db::FinishCommand {
                    command_id: args.command_id,
                    exit_code: args.exit_code,
                    git_repo,
                    git_branch,
                    env_snapshot: Some(&env_snap),
                    stdout: redacted_stdout.as_deref(),
                    stderr: redacted_stderr.as_deref(),
                },
            )?;

            let max_bytes = args.config.capture.max_stdout_bytes;
            let _ = db::compress_command_output_if_needed(conn, args.command_id, max_bytes);
            let _ = db::enforce_retention(conn, args.config.capture.retention_days);
            return Ok(());
        }
        OnDetect::Warn => {
            let stdout_labels: Vec<String> = stdout
                .as_deref()
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                .unwrap_or_default();
            let stderr_labels: Vec<String> = stderr
                .as_deref()
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                .unwrap_or_default();

            if !stdout_labels.is_empty() || !stderr_labels.is_empty() {
                let all_labels: Vec<&str> = stdout_labels
                    .iter()
                    .chain(stderr_labels.iter())
                    .map(|s| s.as_str())
                    .collect();
                eprintln!(
                    "[redtrail] WARN: secrets detected in output ({}), storing unredacted per on_detect=warn",
                    all_labels.join(", ")
                );
            }
        }
    }

    db::finish_command(
        conn,
        &db::FinishCommand {
            command_id: args.command_id,
            exit_code: args.exit_code,
            git_repo,
            git_branch,
            env_snapshot: Some(&env_snap),
            stdout: None, // keep whatever tee wrote
            stderr: None,
        },
    )?;

    let max_bytes = args.config.capture.max_stdout_bytes;
    let _ = db::compress_command_output_if_needed(conn, args.command_id, max_bytes);
    let _ = db::enforce_retention(conn, args.config.capture.retention_days);

    Ok(())
}
