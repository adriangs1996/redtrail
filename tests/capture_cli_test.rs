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

#[test]
fn capture_inserts_command_into_db() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "test-session",
            "--command", "git status",
            "--cwd", "/home/user/project",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--ts-end", "1001",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "capture should succeed. stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Verify it's in the DB
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "git status");
    assert_eq!(cmds[0].exit_code, Some(0));
    assert_eq!(cmds[0].cwd.as_deref(), Some("/home/user/project"));
    assert_eq!(cmds[0].session_id, "test-session");
}

#[test]
fn capture_redacts_secrets() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert!(!cmds[0].command_raw.contains("AKIAIOSFODNN7EXAMPLE"), "secret should be redacted");
    assert!(cmds[0].redacted);
}

#[test]
fn capture_parses_binary_from_command() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "docker build -t myapp .",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds[0].command_binary.as_deref(), Some("docker"));
}

#[test]
fn capture_skips_blacklisted_commands() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "vim src/main.rs",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    // Should succeed silently but NOT insert
    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert!(cmds.is_empty(), "blacklisted command should not be stored");
}

#[test]
fn capture_disabled_via_config_stores_nothing() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "capture:\n  enabled: false\n").unwrap();

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "echo hello",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "should succeed silently even when disabled");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert!(cmds.is_empty(), "disabled capture should not store anything");
}

#[test]
fn capture_custom_blacklist_from_config() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    // Custom blacklist that includes "echo" but NOT "vim"
    std::fs::write(&config_path, "capture:\n  blacklist_commands:\n    - echo\n").unwrap();

    // echo should now be blacklisted
    redtrail_bin()
        .args(["capture", "--session-id", "s1", "--command", "echo hello",
               "--exit-code", "0", "--ts-start", "1000", "--shell", "zsh", "--hostname", "devbox"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert!(cmds.is_empty(), "echo should be blacklisted via config");

    // vim should NOT be blacklisted with custom config (it replaces the default list)
    redtrail_bin()
        .args(["capture", "--session-id", "s1", "--command", "vim src/main.rs",
               "--exit-code", "0", "--ts-start", "1000", "--shell", "zsh", "--hostname", "devbox"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let cmds2 = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds2.len(), 1, "vim should NOT be blacklisted when custom config replaces defaults");
}

#[test]
fn capture_uses_max_stdout_bytes_from_config() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    // Set a tiny max_stdout_bytes
    std::fs::write(&config_path, "capture:\n  max_stdout_bytes: 20\n").unwrap();

    // Write a temp stdout file with content exceeding 20 bytes
    let stdout_file = dir.path().join("stdout.tmp");
    // Tee capture file format: colon headers, blank line, then content
    let long_content = "x".repeat(100);
    std::fs::write(&stdout_file, format!("ts_start:1000\nts_end:1001\ntruncated:false\n\n{long_content}")).unwrap();

    redtrail_bin()
        .args(["capture", "--session-id", "s1", "--command", "echo long output",
               "--exit-code", "0", "--shell", "zsh", "--hostname", "devbox",
               "--stdout-file", stdout_file.to_str().unwrap()])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    let stdout = cmds[0].stdout.as_deref().unwrap_or("");
    assert!(stdout.len() <= 40, "stdout should be truncated to ~20 bytes, got {} bytes", stdout.len());
    assert!(cmds[0].stdout_truncated, "stdout_truncated flag should be set");
}

#[test]
fn capture_on_detect_warn_stores_unredacted_but_flags() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "secrets:\n  on_detect: warn\n").unwrap();

    let output = redtrail_bin()
        .args(["capture", "--session-id", "s1",
               "--command", "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
               "--exit-code", "0", "--ts-start", "1000",
               "--shell", "zsh", "--hostname", "devbox"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    // In warn mode, the secret is stored UNredacted
    assert!(
        cmds[0].command_raw.contains("AKIAIOSFODNN7EXAMPLE"),
        "warn mode should store unredacted command"
    );
    // But the redacted flag is set to indicate detection occurred
    assert!(cmds[0].redacted, "redacted flag should be set in warn mode");

    // Stderr should contain a warning
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("secret") || stderr.contains("WARN") || stderr.contains("warn"),
        "warn mode should emit warning to stderr. Got: {stderr}"
    );
}

#[test]
fn capture_on_detect_block_rejects_command() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(&config_path, "secrets:\n  on_detect: block\n").unwrap();

    let output = redtrail_bin()
        .args(["capture", "--session-id", "s1",
               "--command", "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
               "--exit-code", "0", "--ts-start", "1000",
               "--shell", "zsh", "--hostname", "devbox"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    // Block mode should succeed (no crash) but not store the command
    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert!(cmds.is_empty(), "block mode should not store command with secrets");
}

#[test]
fn capture_custom_patterns_file_detects_custom_secret() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let config_path = dir.path().join("config.yaml");
    let patterns_path = dir.path().join("patterns.yaml");

    // Define a custom pattern that matches "CUSTOMSECRET-" followed by hex
    std::fs::write(&patterns_path, r#"
- label: custom_token
  pattern: "CUSTOMSECRET-[a-f0-9]{16}"
"#).unwrap();

    std::fs::write(&config_path, format!(
        "secrets:\n  patterns_file: {}\n", patterns_path.display()
    )).unwrap();

    let output = redtrail_bin()
        .args(["capture", "--session-id", "s1",
               "--command", "curl -H 'X-Token: CUSTOMSECRET-abcdef0123456789' https://api.example.com",
               "--exit-code", "0", "--ts-start", "1000",
               "--shell", "zsh", "--hostname", "devbox"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert!(
        cmds[0].command_raw.contains("[REDACTED:custom_token]"),
        "custom pattern should redact the token. Got: {}", cmds[0].command_raw
    );
    assert!(!cmds[0].command_raw.contains("CUSTOMSECRET-abcdef0123456789"),
        "custom secret should not be in DB");
}

#[test]
fn capture_is_silent_on_success() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "echo hello",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "capture should produce no stdout");
    assert!(output.stderr.is_empty(), "capture should produce no stderr");
}
