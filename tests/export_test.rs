use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    for i in 0..3 {
        redtrail::core::db::insert_command(&conn, &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: &format!("cmd-{i}"),
            command_binary: Some("echo"),
            exit_code: Some(0),
            timestamp_start: 1000 + i,
            source: "human",
            ..Default::default()
        }).unwrap();
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

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect(&format!("should be valid JSON, got: {stdout}"));
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 3);
}

#[test]
fn export_since_filters_by_timestamp() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["export", "--since", "1001"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 2, "should export 2 commands since ts=1001");
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
