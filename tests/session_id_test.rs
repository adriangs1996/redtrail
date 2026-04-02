use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn session_id_outputs_uuid() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    // Create schema
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["session-id"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let id = stdout.trim();
    assert!(id.len() >= 32, "should be UUID-like, got: {id}");
    assert!(!id.is_empty());
}

#[test]
fn session_id_creates_session_in_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["session-id"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let session = redtrail::core::db::get_session(&conn, &id).unwrap();
    assert_eq!(session.id, id);
}

#[test]
fn session_id_is_silent_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["session-id"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.stderr.is_empty(),
        "should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
