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

#[test]
fn capture_detects_claude_code_from_env() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    redtrail_bin()
        .args([
            "capture",
            "start",
            "--session-id",
            "s1",
            "--command",
            "git status",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .env("CLAUDE_CODE", "1")
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(
        cmds[0].source, "claude_code",
        "should detect Claude Code from CLAUDE_CODE env"
    );
}

#[test]
fn capture_defaults_to_human_without_agent_env() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    redtrail_bin()
        .args([
            "capture",
            "start",
            "--session-id",
            "s1",
            "--command",
            "ls -la",
            "--shell",
            "zsh",
            "--hostname",
            "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        // Explicitly remove agent env vars
        .env_remove("CLAUDE_CODE")
        .env_remove("CLAUDE_CODE_SESSION")
        .env_remove("CURSOR_SESSION_ID")
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds =
        redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default())
            .unwrap();
    assert_eq!(cmds[0].source, "human");
}
