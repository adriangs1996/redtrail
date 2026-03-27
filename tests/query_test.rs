use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db_with_data() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    redtrail::core::db::insert_command(
        &conn,
        &redtrail::core::db::NewCommand {
            session_id: "s1",
            command_raw: "echo hello",
            command_binary: Some("echo"),
            exit_code: Some(0),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    dir
}

#[test]
fn query_select_returns_results() {
    let dir = setup_db_with_data();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["query", "SELECT count(*) as cnt FROM commands"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1"), "should show count of 1, got: {stdout}");
}

#[test]
fn query_non_select_is_rejected() {
    let dir = setup_db_with_data();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["query", "DROP TABLE commands"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        !output.status.success(),
        "DROP should be rejected"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("select"),
        "error should mention SELECT, got: {stderr}"
    );
}

#[test]
fn query_delete_is_rejected() {
    let dir = setup_db_with_data();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["query", "DELETE FROM commands"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(!output.status.success(), "DELETE should be rejected");
}

#[test]
fn query_json_flag() {
    let dir = setup_db_with_data();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args(["query", "--json", "SELECT id, command_raw FROM commands"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect(&format!("should be valid JSON, got: {stdout}"));
    assert!(parsed.is_array());
}
