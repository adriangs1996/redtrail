use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let sid = redtrail::core::db::create_session(
        &conn,
        &redtrail::core::db::NewSession {
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    for i in 0..5 {
        redtrail::core::db::insert_command(
            &conn,
            &redtrail::core::db::NewCommand {
                session_id: &sid,
                command_raw: &format!("cmd-{i}"),
                exit_code: if i == 3 { Some(1) } else { Some(0) },
                timestamp_start: 1000 + i,
                source: "human",
                ..Default::default()
            },
        )
        .unwrap();
    }

    dir
}

#[test]
fn status_shows_command_count() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["status"])
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
        stdout.contains("5"),
        "should show 5 commands, got:\n{stdout}"
    );
}

#[test]
fn status_shows_session_count() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["status"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1"),
        "should show 1 session, got:\n{stdout}"
    );
}

#[test]
fn status_shows_last_capture() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["status"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Last capture:"),
        "should show last capture info, got:\n{stdout}"
    );
}

#[test]
fn status_shows_capture_active() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["status"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Capture:"),
        "should show capture status, got:\n{stdout}"
    );
}

#[test]
fn status_shows_db_size() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["status"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should mention KB or bytes
    assert!(
        stdout.contains("KB") || stdout.contains("MB") || stdout.contains("bytes"),
        "should show DB size, got:\n{stdout}"
    );
}
