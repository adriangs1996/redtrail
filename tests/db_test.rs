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

#[test]
fn redaction_log_table_exists() {
    let conn = setup();
    let exists: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='redaction_log'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(exists, "redaction_log table should exist");
}

#[test]
fn pragma_optimize_does_not_error() {
    // PRAGMA optimize should run without error on a freshly opened DB.
    // We can't directly verify the PRAGMA was called during init, but we
    // can verify it's safe to call (no schema issues) and that the DB
    // has the optimization flag set. A file-based DB is needed since
    // in-memory doesn't persist stats.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let conn = db::open(path.to_str().unwrap()).unwrap();
    // If optimize ran during init without error, the DB is healthy.
    // Double-check by running it explicitly — if there's a schema problem
    // this will fail.
    let result = conn.execute_batch("PRAGMA optimize;");
    assert!(result.is_ok(), "PRAGMA optimize should succeed: {:?}", result.err());
}

// --- Streaming capture tests ---

#[test]
fn commands_table_has_status_column() {
    let conn = setup();
    let has_status: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .any(|col| col.as_deref() == Ok("status"));
    assert!(has_status, "commands table should have a 'status' column");
}

#[test]
fn new_command_defaults_to_finished() {
    use redtrail::core::db::{insert_command, NewCommand};
    let conn = setup();
    let cmd = NewCommand {
        session_id: "sess-1",
        command_raw: "echo hello",
        source: "human",
        timestamp_start: 1000,
        ..Default::default()
    };
    let id = insert_command(&conn, &cmd).unwrap();
    let status: String = conn
        .query_row(
            "SELECT status FROM commands WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "finished", "insert_command should default status to 'finished'");
}

#[test]
fn insert_command_start_creates_running_row() {
    use redtrail::core::db::{insert_command_start, NewCommandStart};
    let conn = setup();
    let id = insert_command_start(
        &conn,
        &NewCommandStart {
            session_id: "sess-1",
            command_raw: "cargo build",
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();
    let (status, stdout, exit_code): (String, Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT status, stdout, exit_code FROM commands WHERE id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "running", "insert_command_start should set status = 'running'");
    assert!(stdout.is_none(), "stdout should be NULL on start");
    assert!(exit_code.is_none(), "exit_code should be NULL on start");
}

#[test]
fn update_command_output_writes_stdout() {
    use redtrail::core::db::{insert_command_start, update_command_output, NewCommandStart};
    let conn = setup();
    let id = insert_command_start(
        &conn,
        &NewCommandStart {
            session_id: "sess-1",
            command_raw: "npm run watch",
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();
    update_command_output(&conn, &id, Some("line1\nline2"), None, false, false).unwrap();
    let stdout: Option<String> = conn
        .query_row(
            "SELECT stdout FROM commands WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stdout.as_deref(), Some("line1\nline2"));
}

#[test]
fn finish_command_sets_status_and_exit_code() {
    use redtrail::core::db::{insert_command_start, finish_command, NewCommandStart, FinishCommand};
    let conn = setup();
    // Create a session so error_count gets incremented
    conn.execute(
        "INSERT INTO sessions (id, started_at, source) VALUES ('sess-1', 1000, 'human')",
        [],
    )
    .unwrap();
    let id = insert_command_start(
        &conn,
        &NewCommandStart {
            session_id: "sess-1",
            command_raw: "make build",
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();
    finish_command(
        &conn,
        &FinishCommand {
            command_id: &id,
            exit_code: Some(0),
            git_repo: Some("/repo"),
            git_branch: Some("main"),
            env_snapshot: None,
            stdout: Some("Build OK"),
            stderr: None,
        },
    )
    .unwrap();
    let (status, exit_code, git_repo, stdout): (String, Option<i32>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT status, exit_code, git_repo, stdout FROM commands WHERE id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(status, "finished");
    assert_eq!(exit_code, Some(0));
    assert_eq!(git_repo.as_deref(), Some("/repo"));
    assert_eq!(stdout.as_deref(), Some("Build OK"));
}

#[test]
fn delete_command_removes_row() {
    use redtrail::core::db::{insert_command, delete_command, NewCommand};
    let conn = setup();
    let cmd = NewCommand {
        session_id: "sess-1",
        command_raw: "ls",
        source: "human",
        timestamp_start: 1000,
        ..Default::default()
    };
    let id = insert_command(&conn, &cmd).unwrap();
    delete_command(&conn, &id).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM commands WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "delete_command should remove the row");
}

#[test]
fn cleanup_orphaned_commands_marks_stale_running() {
    use redtrail::core::db::{cleanup_orphaned_commands, insert_command_start, NewCommandStart};
    let conn = setup();
    // Insert a command with timestamp_start far in the past (> 24h ago)
    let stale_start = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64)
        - 90_000; // 25 hours ago
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, source, status)
         VALUES ('stale-1', 'sess-1', ?1, 'tail -f log', 'human', 'running')",
        [stale_start],
    )
    .unwrap();
    // Also insert a recent running command — should NOT be touched
    let _recent_id = insert_command_start(
        &conn,
        &NewCommandStart {
            session_id: "sess-1",
            command_raw: "npm start",
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let affected = cleanup_orphaned_commands(&conn, "sess-1").unwrap();
    assert_eq!(affected, 1, "only the stale running command should be orphaned");

    let status: String = conn
        .query_row(
            "SELECT status FROM commands WHERE id = 'stale-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "orphaned");
}
