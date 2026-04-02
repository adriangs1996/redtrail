use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    // Just open to create schema
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    dir
}

/// Run `capture start` and return (stdout = command_id, stderr).
fn capture_start(db_path: &str, args: &[&str]) -> std::process::Output {
    let mut full_args = vec!["capture", "start"];
    full_args.extend_from_slice(args);
    redtrail_bin()
        .args(&full_args)
        .env("REDTRAIL_DB", db_path)
        .output()
        .expect("failed to run capture start")
}

/// Run `capture start` with a config override.
fn capture_start_with_config(
    db_path: &str,
    config_path: &str,
    args: &[&str],
) -> std::process::Output {
    let mut full_args = vec!["capture", "start"];
    full_args.extend_from_slice(args);
    redtrail_bin()
        .args(&full_args)
        .env("REDTRAIL_DB", db_path)
        .env("REDTRAIL_CONFIG", config_path)
        .output()
        .expect("failed to run capture start")
}

/// Run `capture finish` and return output.
fn capture_finish(db_path: &str, command_id: &str, exit_code: i32) -> std::process::Output {
    redtrail_bin()
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
        .expect("failed to run capture finish")
}

#[allow(dead_code)]
fn capture_finish_with_config(
    db_path: &str,
    config_path: &str,
    command_id: &str,
    exit_code: i32,
) -> std::process::Output {
    redtrail_bin()
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
        .expect("failed to run capture finish")
}

#[test]
fn capture_start_inserts_running_command() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    let output = capture_start(
        db,
        &[
            "--session-id",
            "test-session",
            "--command",
            "git status",
            "--cwd",
            "/home/user/project",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(
        output.status.success(),
        "capture start should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let command_id = String::from_utf8_lossy(&output.stdout);
    assert!(
        !command_id.is_empty(),
        "capture start should print command ID"
    );

    // Verify it's in the DB with status=running
    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "git status");
    assert_eq!(cmds[0].cwd.as_deref(), Some("/home/user/project"));
    assert_eq!(cmds[0].session_id, "test-session");
    // exit_code should be None (not yet finished)
    assert!(
        cmds[0].exit_code.is_none(),
        "running command should have no exit_code"
    );
}

#[test]
fn capture_start_finish_roundtrip() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    let start_output = capture_start(
        db,
        &[
            "--session-id",
            "test-session",
            "--command",
            "git status",
            "--cwd",
            "/home/user/project",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );
    assert!(start_output.status.success());
    let command_id = String::from_utf8_lossy(&start_output.stdout).to_string();

    let finish_output = capture_finish(db, &command_id, 0);
    assert!(
        finish_output.status.success(),
        "capture finish should succeed. stderr: {}",
        String::from_utf8_lossy(&finish_output.stderr)
    );

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].exit_code, Some(0));
}

#[test]
fn capture_start_redacts_secrets() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    let output = capture_start(
        db,
        &[
            "--session-id",
            "s1",
            "--command",
            "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert!(
        !cmds[0].command_raw.contains("AKIAIOSFODNN7EXAMPLE"),
        "secret should be redacted"
    );
    assert!(cmds[0].redacted);
}

#[test]
fn capture_start_parses_binary_from_command() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    capture_start(
        db,
        &[
            "--session-id",
            "s1",
            "--command",
            "docker build -t myapp .",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds[0].command_binary.as_deref(), Some("docker"));
}

#[test]
fn capture_start_skips_blacklisted_commands() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    let output = capture_start(
        db,
        &[
            "--session-id",
            "s1",
            "--command",
            "vim src/main.rs",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(output.status.success());
    // stdout should be empty (no command ID printed)
    assert!(
        output.stdout.is_empty(),
        "blacklisted command should not print ID"
    );

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert!(cmds.is_empty(), "blacklisted command should not be stored");
}

#[test]
fn capture_start_disabled_via_config_stores_nothing() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "capture:\n  enabled: false\n").unwrap();

    let output = capture_start_with_config(
        db_path.to_str().unwrap(),
        config_path.to_str().unwrap(),
        &[
            "--session-id",
            "s1",
            "--command",
            "echo hello",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(
        output.status.success(),
        "should succeed silently even when disabled"
    );
    assert!(
        output.stdout.is_empty(),
        "disabled capture should not print ID"
    );

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert!(
        cmds.is_empty(),
        "disabled capture should not store anything"
    );
}

#[test]
fn capture_start_custom_blacklist_from_config() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    let db = db_path.to_str().unwrap();
    let cfg = config_path.to_str().unwrap();
    // Custom blacklist that includes "echo" but NOT "vim"
    std::fs::write(
        &config_path,
        "capture:\n  blacklist_commands:\n    - echo\n",
    )
    .unwrap();

    // echo should now be blacklisted
    capture_start_with_config(
        db,
        cfg,
        &[
            "--session-id",
            "s1",
            "--command",
            "echo hello",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert!(cmds.is_empty(), "echo should be blacklisted via config");

    // vim should NOT be blacklisted with custom config (it replaces the default list)
    capture_start_with_config(
        db,
        cfg,
        &[
            "--session-id",
            "s1",
            "--command",
            "vim src/main.rs",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    let cmds2 =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(
        cmds2.len(),
        1,
        "vim should NOT be blacklisted when custom config replaces defaults"
    );
}

#[test]
fn capture_start_on_detect_warn_stores_unredacted_but_flags() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "secrets:\n  on_detect: warn\n").unwrap();
    let db = db_path.to_str().unwrap();
    let cfg = config_path.to_str().unwrap();

    let output = capture_start_with_config(
        db,
        cfg,
        &[
            "--session-id",
            "s1",
            "--command",
            "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert!(
        cmds[0].command_raw.contains("AKIAIOSFODNN7EXAMPLE"),
        "warn mode should store unredacted command"
    );

    // Stderr should contain a warning
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("secret") || stderr.contains("WARN") || stderr.contains("warn"),
        "warn mode should emit warning to stderr. Got: {stderr}"
    );
}

#[test]
fn capture_start_on_detect_block_rejects_command() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "secrets:\n  on_detect: block\n").unwrap();
    let db = db_path.to_str().unwrap();
    let cfg = config_path.to_str().unwrap();

    let output = capture_start_with_config(
        db,
        cfg,
        &[
            "--session-id",
            "s1",
            "--command",
            "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "block mode should not print ID");

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert!(
        cmds.is_empty(),
        "block mode should not store command with secrets"
    );
}

#[test]
fn capture_start_custom_patterns_file_detects_custom_secret() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    let patterns_path = dir.path().join("patterns.yaml");
    let db = db_path.to_str().unwrap();
    let cfg = config_path.to_str().unwrap();

    std::fs::write(
        &patterns_path,
        r#"
- label: custom_token
  pattern: "CUSTOMSECRET-[a-f0-9]{16}"
"#,
    )
    .unwrap();

    std::fs::write(
        &config_path,
        format!("secrets:\n  patterns_file: {}\n", patterns_path.display()),
    )
    .unwrap();

    let output = capture_start_with_config(
        db,
        cfg,
        &[
            "--session-id",
            "s1",
            "--command",
            "curl -H 'X-Token: CUSTOMSECRET-abcdef0123456789' https://api.example.com",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert!(
        cmds[0].command_raw.contains("[REDACTED:custom_token]"),
        "custom pattern should redact the token. Got: {}",
        cmds[0].command_raw
    );
    assert!(
        !cmds[0]
            .command_raw
            .contains("CUSTOMSECRET-abcdef0123456789"),
        "custom secret should not be in DB"
    );
}

#[test]
fn capture_start_is_silent_on_success_except_id() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    let output = capture_start(
        db,
        &[
            "--session-id",
            "s1",
            "--command",
            "echo hello",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ],
    );

    assert!(output.status.success());
    // stdout should contain only the command ID (no newline, no extra output)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "capture start should print command ID");
    assert!(
        !stdout.contains('\n'),
        "capture start stdout should be just the ID"
    );
    assert!(
        output.stderr.is_empty(),
        "capture start should produce no stderr"
    );
}

#[test]
fn capture_finish_nonexistent_command_is_noop() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let db = db_path.to_str().unwrap();

    let output = capture_finish(db, "nonexistent-id", 0);
    assert!(
        output.status.success(),
        "finish on nonexistent ID should succeed silently"
    );
}
