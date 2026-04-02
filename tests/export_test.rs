use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    // One ancient command
    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "echo old",
            command_binary: Some("echo"),
            exit_code: Some(0),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Two recent commands
    let now = now_ts();
    for i in 0..2 {
        redtrail::core::db::insert_command(
            &conn,
            &redtrail::core::db::NewCommand {
                session_id: "s1",
                command_raw: &format!("echo recent-{i}"),
                command_binary: Some("echo"),
                exit_code: Some(0),
                timestamp_start: now - 60 + i, // within the last minute
                source: "human",
                ..Default::default()
            },
        )
        .unwrap();
    }

    dir
}

#[test]
fn export_outputs_valid_json() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["export"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect(&format!("should be valid JSON, got: {stdout}"));
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 3);
}

#[test]
fn export_since_filters_by_duration() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    // Export only the last 5 minutes — should get the 2 recent commands but not the ancient one
    let output = redtrail_bin()
        .args(["export", "--since", "5m"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 2, "should export 2 recent commands");
}

#[test]
fn export_empty_db_returns_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["export"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}
