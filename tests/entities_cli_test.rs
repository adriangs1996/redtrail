use redtrail::core::db;
use redtrail::extract;

fn setup_with_entities() -> rusqlite::Connection {
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
    extract::extract_command(&conn, &cmd).unwrap();
    conn
}

#[test]
fn entities_lists_all_types() {
    let conn = setup_with_entities();

    let args = redtrail::cmd::entities::EntitiesArgs {
        entity_type: None,
        json: false,
    };
    // Should not panic
    redtrail::cmd::entities::run(&conn, &args).unwrap();
}

#[test]
fn entities_filter_by_type() {
    let conn = setup_with_entities();

    let args = redtrail::cmd::entities::EntitiesArgs {
        entity_type: Some("git_branch"),
        json: false,
    };
    redtrail::cmd::entities::run(&conn, &args).unwrap();
}

#[test]
fn entities_json_output_is_valid() {
    let conn = setup_with_entities();

    // Capture output by calling the function (it prints to stdout; we verify no panic + entity count)
    let entities = extract::db::get_entities(
        &conn,
        &extract::db::EntityFilter {
            entity_type: Some("git_branch"),
            limit: None,
        },
    )
    .unwrap();

    // At least one branch should be present (main and/or dev)
    assert!(!entities.is_empty(), "should have git_branch entities");

    let args = redtrail::cmd::entities::EntitiesArgs {
        entity_type: Some("git_branch"),
        json: true,
    };
    redtrail::cmd::entities::run(&conn, &args).unwrap();
}

#[test]
fn entities_empty_db_does_not_error() {
    let conn = db::open_in_memory().unwrap();
    let args = redtrail::cmd::entities::EntitiesArgs {
        entity_type: None,
        json: false,
    };
    redtrail::cmd::entities::run(&conn, &args).unwrap();
}

// --- CLI binary integration ---

fn redtrail_bin() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn entities_command_succeeds_via_binary() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    // Empty DB — should succeed with "No entities found."
    db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["entities"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run redtrail entities");

    assert!(
        output.status.success(),
        "entities should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn entities_json_flag_via_binary() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    db::open(db_path.to_str().unwrap()).unwrap();

    let output = redtrail_bin()
        .args(["entities", "--json"])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("should be valid JSON, got: {stdout}"));
    assert!(parsed.is_array());
}
