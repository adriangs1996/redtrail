use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn schema_creates_all_core_tables() {
    let conn = setup();
    let expected_tables = [
        "commands",
        "sessions",
        "entities",
        "relationships",
    ];
    for table in &expected_tables {
        let exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert!(exists, "table '{table}' should exist");
    }
}

#[test]
fn schema_creates_pattern_mining_tables() {
    let conn = setup();
    let expected_tables = [
        "patterns",
        "error_resolutions",
        "suggestions",
    ];
    for table in &expected_tables {
        let exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert!(exists, "table '{table}' should exist");
    }
}

#[test]
fn schema_creates_fts_index() {
    let conn = setup();
    let exists: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='commands_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(exists, "FTS virtual table 'commands_fts' should exist");
}

#[test]
fn wal_mode_enabled() {
    let conn = setup();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    // In-memory DBs return "memory" for journal_mode, so test with a real file
    // This test validates the PRAGMA is issued; file-based test below covers WAL.
    assert!(
        mode == "wal" || mode == "memory",
        "expected wal or memory, got: {mode}"
    );
}

#[test]
fn wal_mode_on_file_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let conn = db::open(path.to_str().unwrap()).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn foreign_keys_enabled() {
    let conn = setup();
    let fk: i32 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1, "foreign_keys should be enabled");
}

#[test]
fn commands_table_has_required_columns() {
    let conn = setup();
    // Insert a minimal command row to verify columns exist
    let result = conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, cwd, hostname, shell, source)
         VALUES ('cmd-1', 'sess-1', 1234567890, 'echo hello', 'echo', '/tmp', 'localhost', 'zsh', 'human')",
        [],
    );
    assert!(result.is_ok(), "insert should succeed: {:?}", result.err());

    // Verify nullable agent-awareness columns exist
    let result2 = conn.execute(
        "UPDATE commands SET agent_session_id = 'abc', parent_process = 'node', is_automated = 1 WHERE id = 'cmd-1'",
        [],
    );
    assert!(result2.is_ok(), "agent columns should exist: {:?}", result2.err());
}

#[test]
fn sessions_table_has_required_columns() {
    let conn = setup();
    let result = conn.execute(
        "INSERT INTO sessions (id, started_at, source)
         VALUES ('sess-1', 1234567890, 'human')",
        [],
    );
    assert!(result.is_ok(), "insert should succeed: {:?}", result.err());
}
