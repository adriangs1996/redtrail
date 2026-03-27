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
}

pub fn run(conn: &Connection, args: &CaptureArgs) -> Result<(), Error> {
    let parsed = capture::parse_command(args.command);

    let blacklist = capture::default_blacklist();
    if capture::is_blacklisted(&parsed.binary, &blacklist) {
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

    db::insert_command_redacted(
        conn,
        &db::NewCommand {
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
        },
    )?;

    // Clean up temp files
    if let Some(path) = args.stdout_file {
        let _ = std::fs::remove_file(path);
    }
    if let Some(path) = args.stderr_file {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}
