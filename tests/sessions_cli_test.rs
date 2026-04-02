use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let sid = redtrail::core::db::create_session(
        &conn,
        &redtrail::core::db::NewSession {
            cwd_initial: Some("/home/user/project"),
            hostname: Some("devbox"),
            shell: Some("zsh"),
            source: "human",
        },
    )
    .unwrap();

    for i in 0..3 {
        redtrail::core::db::insert_command(
            &conn,
            &redtrail::core::db::NewCommand {
                session_id: &sid,
                command_raw: &format!("cmd-{i}"),
                exit_code: Some(0),
                timestamp_start: 1000 + i,
                source: "human",
                ..Default::default()
            },
        )
        .unwrap();
    }

    (dir, sid)
}

#[test]
fn sessions_lists_all_sessions() {
    let (dir, sid) = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["sessions"])
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
        stdout.contains(&sid[..8]),
        "should show session ID (at least prefix), got:\n{stdout}"
    );
}

#[test]
fn session_detail_shows_commands() {
    let (dir, sid) = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["session", &sid])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cmd-0"), "should show commands in session");
    assert!(stdout.contains("cmd-1"), "should show commands in session");
    assert!(stdout.contains("cmd-2"), "should show commands in session");
}

#[test]
fn session_invalid_id_fails() {
    let (dir, _sid) = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["session", "nonexistent-id"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(!output.status.success(), "invalid session should fail");
}
