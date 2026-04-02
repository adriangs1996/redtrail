use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

/// Helper: create a temp DB, insert some commands, return the path.
fn setup_db_with_commands() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "git status",
            command_binary: Some("git"),
            cwd: Some("/home/user/project"),
            exit_code: Some(0),
            hostname: Some("devbox"),
            shell: Some("zsh"),
            source: "human",
            timestamp_start: 1000,
            ..Default::default()
        },
    )
    .unwrap();

    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "cargo build",
            command_binary: Some("cargo"),
            cwd: Some("/home/user/project"),
            exit_code: Some(1),
            stderr: Some("error[E0308]: mismatched types"),
            hostname: Some("devbox"),
            shell: Some("zsh"),
            source: "human",
            timestamp_start: 1001,
            ..Default::default()
        },
    )
    .unwrap();

    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "echo hello",
            command_binary: Some("echo"),
            cwd: Some("/tmp"),
            exit_code: Some(0),
            hostname: Some("devbox"),
            shell: Some("zsh"),
            source: "human",
            timestamp_start: 1002,
            ..Default::default()
        },
    )
    .unwrap();

    dir
}

#[test]
fn history_lists_commands() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail");

    assert!(
        output.status.success(),
        "history should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("git status"),
        "should show git status command"
    );
    assert!(
        stdout.contains("cargo build"),
        "should show cargo build command"
    );
    assert!(stdout.contains("echo hello"), "should show echo command");
}

#[test]
fn history_failed_only() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--failed"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cargo build"), "should show failed command");
    assert!(
        !stdout.contains("git status"),
        "should NOT show successful commands"
    );
    assert!(
        !stdout.contains("echo hello"),
        "should NOT show successful commands"
    );
}

#[test]
fn history_filter_by_cmd() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--cmd", "git"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("git status"), "should show git commands");
    assert!(
        !stdout.contains("cargo build"),
        "should NOT show non-git commands"
    );
}

#[test]
fn history_json_output() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--json"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect(&format!("output should be valid JSON, got: {stdout}"));
    assert!(parsed.is_array(), "JSON output should be an array");

    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn history_empty_result_is_not_error() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--cmd", "nonexistent"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail");

    assert!(
        output.status.success(),
        "empty result should still be exit 0"
    );
}

#[test]
fn history_search_finds_in_command() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--search", "status"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("git status"),
        "should find 'status' in command_raw"
    );
    assert!(
        !stdout.contains("cargo build"),
        "should NOT show non-matching"
    );
}

#[test]
fn history_search_finds_in_stderr() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--search", "mismatched"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("cargo build"),
        "should find 'mismatched' in stderr of cargo build"
    );
}

#[test]
fn history_verbose_shows_stdout_stderr() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    // Insert a command with stdout and stderr
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "make build",
            command_binary: Some("make"),
            exit_code: Some(2),
            stdout: Some("compiling main.c"),
            stderr: Some("error: undefined reference"),
            source: "human",
            timestamp_start: 1003,
            ..Default::default()
        },
    )
    .unwrap();
    drop(conn);

    let output = redtrail_bin()
        .args(["history", "--verbose"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("compiling main.c"),
        "verbose should show stdout"
    );
    assert!(
        stdout.contains("error: undefined reference"),
        "verbose should show stderr"
    );
}

#[test]
fn history_auto_json_when_piped() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    // When output is piped (not a TTY), it should auto-switch to JSON.
    // In tests, stdout is already not a TTY, so plain `history` should produce JSON.
    let output = redtrail_bin()
        .args(["history"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        parsed.is_ok(),
        "piped output should be valid JSON, got:\n{stdout}"
    );
    assert!(parsed.unwrap().is_array());
}

#[test]
fn history_cwd_dot_resolves_to_current_dir() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    // Insert a command with cwd set to the canonicalized temp dir
    let canon = dir.path().canonicalize().unwrap();
    let cwd = canon.to_str().unwrap();
    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "ls -la",
            command_binary: Some("ls"),
            cwd: Some(cwd),
            exit_code: Some(0),
            source: "human",
            timestamp_start: 2000,
            ..Default::default()
        },
    )
    .unwrap();

    // Insert a command with a different cwd
    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "pwd",
            command_binary: Some("pwd"),
            cwd: Some("/some/other/dir"),
            exit_code: Some(0),
            source: "human",
            timestamp_start: 2001,
            ..Default::default()
        },
    )
    .unwrap();
    drop(conn);

    // Run with --cwd . from the temp dir
    let output = redtrail_bin()
        .args(["history", "--cwd", "."])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .current_dir(dir.path())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ls -la"),
        "should show command in current dir"
    );
    assert!(
        !stdout.contains("pwd"),
        "should NOT show command from different dir"
    );
}

#[test]
fn history_search_no_match() {
    let dir = setup_db_with_commands();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["history", "--search", "kubernetes"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "no match should still be exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty() || !stdout.contains("git"),
        "should return empty"
    );
}
