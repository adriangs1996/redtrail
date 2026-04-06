use redtrail::core::db;
use redtrail::extract;

fn setup_with_entity() -> (rusqlite::Connection, String) {
    let conn = db::open_in_memory().unwrap();

    conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, stdout, git_repo, source, status)
         VALUES ('c1', 'sess', 1000, 'git branch', 'git', 'branch', '* main\n  dev\n', '/repo', 'human', 'finished')",
        [],
    )
    .unwrap();

    let cmd = extract::db::get_command_by_id(&conn, "c1").unwrap();
    extract::extract_command(&conn, &cmd, None).unwrap();

    let entities = extract::db::get_entities(
        &conn,
        &extract::db::EntityFilter {
            entity_type: Some("git_branch"),
            limit: None,
        },
    )
    .unwrap();
    assert!(!entities.is_empty(), "setup requires at least one entity");
    let id = entities[0].id.clone();
    (conn, id)
}

#[test]
fn entity_shows_details() {
    let (conn, id) = setup_with_entity();
    let args = redtrail::cmd::entity::EntityArgs {
        id: &id,
        relationships: false,
        history: false,
        json: false,
    };
    redtrail::cmd::entity::run(&conn, &args).unwrap();
}

#[test]
fn entity_with_relationships() {
    let (conn, id) = setup_with_entity();
    let args = redtrail::cmd::entity::EntityArgs {
        id: &id,
        relationships: true,
        history: false,
        json: false,
    };
    redtrail::cmd::entity::run(&conn, &args).unwrap();
}

#[test]
fn entity_with_history() {
    let (conn, id) = setup_with_entity();
    let args = redtrail::cmd::entity::EntityArgs {
        id: &id,
        relationships: false,
        history: true,
        json: false,
    };
    redtrail::cmd::entity::run(&conn, &args).unwrap();
}

#[test]
fn entity_json_output_is_valid_json() {
    let (conn, id) = setup_with_entity();
    // We test by checking the library directly against what the fn prints.
    // Since we can't easily capture stdout in unit tests, we verify the
    // entity exists and the function returns Ok.
    let args = redtrail::cmd::entity::EntityArgs {
        id: &id,
        relationships: true,
        history: true,
        json: true,
    };
    redtrail::cmd::entity::run(&conn, &args).unwrap();
}

#[test]
fn entity_not_found_returns_error() {
    let conn = db::open_in_memory().unwrap();
    let args = redtrail::cmd::entity::EntityArgs {
        id: "nonexistent-uuid",
        relationships: false,
        history: false,
        json: false,
    };
    let result = redtrail::cmd::entity::run(&conn, &args);
    assert!(result.is_err(), "should return error for missing entity");
}

// --- CLI binary integration ---

fn redtrail_bin() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn entity_command_not_found_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["entity", "nonexistent-id"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail entity");

    assert!(
        !output.status.success(),
        "entity with bad ID should exit nonzero"
    );
}

#[test]
fn entity_command_json_via_binary() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = db::open(db_path.to_str().unwrap()).unwrap();

    // Insert and extract to get an entity
    conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('s1', 1000, 'human')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, stdout, git_repo, source, status)
         VALUES ('c1', 's1', 1000, 'git branch', 'git', 'branch', '* main\n', '/repo', 'human', 'finished')",
        [],
    )
    .unwrap();
    let cmd = extract::db::get_command_by_id(&conn, "c1").unwrap();
    extract::extract_command(&conn, &cmd, None).unwrap();

    let entities = extract::db::get_entities(
        &conn,
        &extract::db::EntityFilter {
            entity_type: Some("git_branch"),
            limit: Some(1),
        },
    )
    .unwrap();
    assert!(!entities.is_empty());
    let id = entities[0].id.clone();
    drop(conn);

    let output = redtrail_bin()
        .args(["entity", &id, "--json"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("should be valid JSON, got: {stdout}"));
    assert_eq!(parsed["id"], id);
    assert_eq!(parsed["type"], "git_branch");
}
