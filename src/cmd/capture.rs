use crate::core::capture;
use crate::core::db;
use crate::core::tee;
use crate::error::Error;
use rusqlite::Connection;

pub struct CaptureArgs<'a> {
    pub session_id: &'a str,
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub ts_start: Option<i64>,
    pub ts_end: Option<i64>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub stdout_file: Option<&'a str>,
    pub stderr_file: Option<&'a str>,
    pub config: Option<&'a crate::config::Config>,
}

pub fn run(conn: &Connection, args: &CaptureArgs) -> Result<(), Error> {
    let default_config = crate::config::Config::default();
    let config = args.config.unwrap_or(&default_config);

    if !config.capture.enabled {
        return Ok(());
    }

    let parsed = capture::parse_command(args.command);

    if capture::is_blacklisted(&parsed.binary, &config.capture.blacklist_commands) {
        return Ok(());
    }

    // Read stdout/stderr from temp files if provided
    let stdout_capture = args
        .stdout_file
        .and_then(|p| tee::read_capture_file(std::path::Path::new(p)));
    let stderr_capture = args
        .stderr_file
        .and_then(|p| tee::read_capture_file(std::path::Path::new(p)));

    // Use timestamps from temp file headers if available, else fall back to CLI args, else now
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let ts_start = stdout_capture
        .as_ref()
        .map(|(h, _)| h.ts_start)
        .or(stderr_capture.as_ref().map(|(h, _)| h.ts_start))
        .or(args.ts_start)
        .unwrap_or(now_secs);

    let ts_end = stdout_capture
        .as_ref()
        .map(|(h, _)| h.ts_end)
        .or(stderr_capture.as_ref().map(|(h, _)| h.ts_end))
        .or(args.ts_end);

    let max_bytes = config.capture.max_stdout_bytes;

    // Pass full content — compression happens at the DB layer for over-limit output
    let stdout_content = stdout_capture.as_ref().map(|(_, c)| c.as_str());
    let stderr_content = stderr_capture.as_ref().map(|(_, c)| c.as_str());
    let stdout_truncated = stdout_capture
        .as_ref()
        .is_some_and(|(h, _)| h.truncated);
    let stderr_truncated = stderr_capture
        .as_ref()
        .is_some_and(|(h, _)| h.truncated);

    let git = args.cwd.map(capture::git_context);
    let git_repo = git.as_ref().and_then(|g| g.repo.as_deref());
    let git_branch = git.as_ref().and_then(|g| g.branch.as_deref());

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let env_snap = capture::env_snapshot(&env);
    let source = capture::detect_source(&env, None);

    let args_json = serde_json::to_string(&parsed.args).unwrap_or_default();
    let flags_json = serde_json::to_string(&parsed.flags).unwrap_or_default();

    use crate::config::OnDetect;
    use crate::core::secrets::engine::{load_custom_patterns, redact_with_custom_patterns, CustomPattern};

    let custom_patterns: Vec<CustomPattern> = config
        .secrets
        .patterns_file
        .as_deref()
        .map(load_custom_patterns)
        .unwrap_or_default();

    let base_cmd = db::NewCommand {
        session_id: args.session_id,
        command_raw: args.command,
        command_binary: if parsed.binary.is_empty() {
            None
        } else {
            Some(&parsed.binary)
        },
        command_subcommand: parsed.subcommand.as_deref(),
        command_args: Some(&args_json),
        command_flags: Some(&flags_json),
        cwd: args.cwd,
        git_repo,
        git_branch,
        exit_code: args.exit_code,
        stdout: stdout_content,
        stderr: stderr_content,
        stdout_truncated,
        stderr_truncated,
        timestamp_start: ts_start,
        timestamp_end: ts_end,
        shell: args.shell,
        hostname: args.hostname,
        env_snapshot: Some(&env_snap),
        source,
        ..Default::default()
    };

    match config.secrets.on_detect {
        OnDetect::Redact => {
            db::insert_command_redacted_compressed(conn, &base_cmd, &custom_patterns, max_bytes)?;
        }
        OnDetect::Warn => {
            // Check for secrets but store unredacted
            let (_, raw_labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            let stdout_labels: Vec<String> = stdout_content
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                .unwrap_or_default();
            let stderr_labels: Vec<String> = stderr_content
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                .unwrap_or_default();
            let has_secrets = !raw_labels.is_empty()
                || !stdout_labels.is_empty()
                || !stderr_labels.is_empty();

            if has_secrets {
                let all_labels: Vec<&str> = raw_labels.iter()
                    .chain(stdout_labels.iter())
                    .chain(stderr_labels.iter())
                    .map(|s| s.as_str())
                    .collect();
                eprintln!(
                    "[redtrail] WARN: secrets detected ({}), storing unredacted per on_detect=warn",
                    all_labels.join(", ")
                );
            }

            let cmd = db::NewCommand {
                redacted: has_secrets,
                ..base_cmd
            };
            db::insert_command_compressed(conn, &cmd, max_bytes)?;
        }
        OnDetect::Block => {
            // Check for secrets — if found, refuse to store
            let (_, raw_labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            let stdout_labels: Vec<String> = stdout_content
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                .unwrap_or_default();
            let stderr_labels: Vec<String> = stderr_content
                .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                .unwrap_or_default();
            let has_secrets = !raw_labels.is_empty()
                || !stdout_labels.is_empty()
                || !stderr_labels.is_empty();

            if has_secrets {
                // Don't store — secrets would leak
                return Ok(());
            }
            db::insert_command_compressed(conn, &base_cmd, max_bytes)?;
        }
    };

    // Clean up temp files
    if let Some(path) = args.stdout_file {
        let _ = std::fs::remove_file(path);
    }
    if let Some(path) = args.stderr_file {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}
