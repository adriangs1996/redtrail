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
