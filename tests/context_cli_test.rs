use redtrail::core::db;
use redtrail::extract;

fn setup_with_git_entities() -> rusqlite::Connection {
    let conn = db::open_in_memory().unwrap();

    conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    )
    .unwrap();

    // git branch output
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, stdout, git_repo, source, status)
         VALUES ('c1', 'sess', 1000, 'git branch', 'git', 'branch', '* main\n  feature/x\n', '/myrepo', 'human', 'finished')",
        [],
    )
    .unwrap();

    // git status output
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, stdout, git_repo, source, status)
         VALUES ('c2', 'sess', 1001, 'git status', 'git', 'status', ' M src/lib.rs\n?? new.txt\n', '/myrepo', 'human', 'finished')",
        [],
    )
    .unwrap();

    // A failed command
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, exit_code, git_repo, source, status)
         VALUES ('c3', 'sess', 1002, 'cargo build', 'cargo', 1, '/myrepo', 'human', 'finished')",
        [],
    )
    .unwrap();

    for id in &["c1", "c2"] {
        let cmd = extract::db::get_command_by_id(&conn, id).unwrap();
        extract::extract_command(&conn, &cmd, None).unwrap();
    }

    conn
}

#[test]
fn context_markdown_does_not_crash() {
    let conn = setup_with_git_entities();
    let args = redtrail::cmd::context::ContextArgs {
        format: "markdown",
        repo: Some("/myrepo"),
    };
    redtrail::cmd::context::run(&conn, &args).unwrap();
}

#[test]
fn context_json_does_not_crash() {
    let conn = setup_with_git_entities();
    let args = redtrail::cmd::context::ContextArgs {
        format: "json",
        repo: Some("/myrepo"),
    };
    redtrail::cmd::context::run(&conn, &args).unwrap();
}

#[test]
fn context_empty_db_does_not_crash() {
    let conn = db::open_in_memory().unwrap();
    let args = redtrail::cmd::context::ContextArgs {
        format: "markdown",
        repo: Some("/nonexistent"),
    };
    redtrail::cmd::context::run(&conn, &args).unwrap();
}

#[test]
fn context_no_repo_arg_does_not_crash() {
    let conn = setup_with_git_entities();
    // Without a repo arg, detect_repo() will run git in cwd — it may or may not
    // find a repo. Either way the command must not panic.
    let args = redtrail::cmd::context::ContextArgs {
        format: "markdown",
        repo: None,
    };
    redtrail::cmd::context::run(&conn, &args).unwrap();
}

// --- CLI binary integration ---

fn redtrail_bin() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn context_command_succeeds_via_binary() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["context"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail context");

    assert!(
        output.status.success(),
        "context should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Project Context"),
        "markdown output should have a title"
    );
}

#[test]
fn context_json_format_via_binary() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["context", "--format", "json"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("should be valid JSON, got: {stdout}"));
    assert!(parsed.is_object(), "json output should be an object");
    assert!(parsed.get("branches").is_some());
    assert!(parsed.get("recent_commits").is_some());
    assert!(parsed.get("recent_errors").is_some());
}

#[test]
fn context_repo_flag_via_binary() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["context", "--repo", "/some/path"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
