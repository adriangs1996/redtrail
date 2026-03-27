use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let sid = redtrail::core::db::create_session(&conn, &redtrail::core::db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    let cmd_id = redtrail::core::db::insert_command(&conn, &redtrail::core::db::NewCommand {
        session_id: &sid,
        command_raw: "echo hello",
        timestamp_start: 1000,
        source: "human",
        ..Default::default()
    }).unwrap();

    redtrail::core::db::insert_command(&conn, &redtrail::core::db::NewCommand {
        session_id: &sid,
        command_raw: "echo world",
        timestamp_start: 2000,
        source: "human",
        ..Default::default()
    }).unwrap();

    (dir, sid, cmd_id)
}

#[test]
fn forget_command_via_cli() {
    let (dir, _sid, cmd_id) = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["forget", "--command", &cmd_id])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1, "one command should remain");
    assert_eq!(cmds[0].command_raw, "echo world");
}

#[test]
fn forget_session_via_cli() {
    let (dir, sid, _cmd_id) = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["forget", "--session", &sid])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert!(cmds.is_empty(), "all commands should be deleted");
}

#[test]
fn forget_last_duration_via_cli() {
    let (dir, _sid, _cmd_id) = setup_db();
    let db_path = dir.path().join("test.db");

    // Delete everything from the last 500s (should delete the ts=2000 one since we pass --last with a relative duration)
    // Actually, let's use a concrete since timestamp to keep it deterministic
    let output = redtrail_bin()
        .args(["forget", "--since", "1500"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(&conn, &redtrail::core::db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "echo hello");
}
