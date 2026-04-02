use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn session_command_count_increments_on_insert() {
    let conn = setup();

    let sid = db::create_session(
        &conn,
        &db::NewSession {
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Before any commands
    let session = db::get_session(&conn, &sid).unwrap();
    assert_eq!(session.command_count, 0);

    // Insert two commands
    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: &sid,
            command_raw: "echo 1",
            exit_code: Some(0),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: &sid,
            command_raw: "false",
            exit_code: Some(1),
            timestamp_start: 1001,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let session = db::get_session(&conn, &sid).unwrap();
    assert_eq!(session.command_count, 2, "should count both commands");
    assert_eq!(session.error_count, 1, "should count the failed command");
}

#[test]
fn file_db_has_restrictive_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let _conn = db::open(path.to_str().unwrap()).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "DB file should be 600 (owner rw only), got: {mode:o}"
        );
    }
}
