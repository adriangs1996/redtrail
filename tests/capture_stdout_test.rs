/// Tests for capture start/finish with stdout/stderr written directly to DB.
///
/// In the new architecture, tee writes stdout/stderr to DB via `update_command_output`.
/// The `capture finish` command reads them back for final secret redaction and compression.
use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    dir
}

/// Helper: run `capture start` and return the command ID.
fn start_command(db_path: &str, session: &str, command: &str) -> String {
    let output = redtrail_bin()
        .args([
            "capture",
            "start",
            "--session-id",
            session,
            "--command",
            command,
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ])
        .env("REDTRAIL_DB", db_path)
        .output()
        .expect("failed to run capture start");
    assert!(
        output.status.success(),
        "capture start failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Helper: simulate tee writing stdout/stderr to DB.
fn write_output_to_db(db_path: &str, command_id: &str, stdout: Option<&str>, stderr: Option<&str>) {
    let conn = redtrail::core::db::open(db_path).unwrap();
    redtrail::core::db::update_command_output(&conn, command_id, stdout, stderr, false, false)
        .unwrap();
}

/// Helper: run `capture finish`.
fn finish_command(db_path: &str, command_id: &str, exit_code: i32) {
    let output = redtrail_bin()
        .args([
            "capture",
            "finish",
            "--command-id",
            command_id,
            "--exit-code",
            &exit_code.to_string(),
        ])
        .env("REDTRAIL_DB", db_path)
        .output()
        .expect("failed to run capture finish");
    assert!(
        output.status.success(),
        "capture finish failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn finish_command_with_config(db_path: &str, config_path: &str, command_id: &str, exit_code: i32) {
    let output = redtrail_bin()
        .args([
            "capture",
            "finish",
            "--command-id",
            command_id,
            "--exit-code",
            &exit_code.to_string(),
        ])
        .env("REDTRAIL_DB", db_path)
        .env("REDTRAIL_CONFIG", config_path)
        .output()
        .expect("failed to run capture finish");
    assert!(
        output.status.success(),
        "capture finish failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn finish_preserves_stdout_written_by_tee() {
    let dir = setup_db();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    let id = start_command(db_path, "s1", "echo hello");
    write_output_to_db(db_path, &id, Some("hello from stdout\n"), None);
    finish_command(db_path, &id, 0);

    let conn = redtrail::core::db::open(db_path).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].stdout.as_deref(), Some("hello from stdout\n"));
    assert_eq!(cmds[0].exit_code, Some(0));
}

#[test]
fn finish_preserves_stderr_written_by_tee() {
    let dir = setup_db();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    let id = start_command(db_path, "s1", "make build");
    write_output_to_db(db_path, &id, None, Some("error output\n"));
    finish_command(db_path, &id, 1);

    let conn = redtrail::core::db::open(db_path).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds[0].stderr.as_deref(), Some("error output\n"));
    assert_eq!(cmds[0].exit_code, Some(1));
}

#[test]
fn finish_without_output_still_works() {
    let dir = setup_db();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    let id = start_command(db_path, "s1", "ls");
    finish_command(db_path, &id, 0);

    let conn = redtrail::core::db::open(db_path).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].stdout.is_none());
    assert!(cmds[0].stderr.is_none());
}

#[test]
fn finish_redacts_secrets_in_stdout_with_redact_mode() {
    let dir = setup_db();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    let id = start_command(db_path, "s1", "cat credentials");
    write_output_to_db(
        db_path,
        &id,
        Some("aws_access_key_id=AKIAIOSFODNN7EXAMPLE\n"),
        None,
    );

    // Default on_detect is Redact, so finish should redact
    finish_command(db_path, &id, 0);

    let conn = redtrail::core::db::open(db_path).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    let stdout = cmds[0].stdout.as_ref().unwrap();
    assert!(
        !stdout.contains("AKIAIOSFODNN7EXAMPLE"),
        "secret should be redacted in stdout"
    );
}

#[test]
fn finish_deletes_row_with_secrets_in_block_mode() {
    let dir = setup_db();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "secrets:\n  on_detect: block\n").unwrap();

    // Use start with block config — command itself has no secrets
    let output = redtrail_bin()
        .args([
            "capture",
            "start",
            "--session-id",
            "s1",
            "--command",
            "cat credentials",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ])
        .env("REDTRAIL_DB", db_path)
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    let id = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(!id.is_empty());

    // Tee writes secret content
    write_output_to_db(db_path, &id, Some("AKIAIOSFODNN7EXAMPLE\n"), None);

    // Finish with block mode should delete the row
    finish_command_with_config(db_path, config_path.to_str().unwrap(), &id, 0);

    let conn = redtrail::core::db::open(db_path).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert!(
        cmds.is_empty(),
        "block mode should delete command when stdout contains secrets"
    );
}
