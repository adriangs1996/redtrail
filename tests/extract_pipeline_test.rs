use redtrail::core::db;
use redtrail::extract;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn insert_git_command(conn: &rusqlite::Connection, id: &str, subcommand: &str, stdout: &str) {
    // Need a session first for FK
    let _ = conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    );
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, stdout, git_repo, source, status)
         VALUES (?1, 'sess', 1000, ?2, 'git', ?3, ?4, '/home/user/project', 'human', 'finished')",
        rusqlite::params![id, format!("git {subcommand}"), subcommand, stdout],
    ).unwrap();
}

fn insert_generic_command(conn: &rusqlite::Connection, id: &str, binary: &str, stdout: &str) {
    let _ = conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    );
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, stdout, source, status)
         VALUES (?1, 'sess', 1000, ?2, ?3, ?4, 'human', 'finished')",
        rusqlite::params![id, binary, binary, stdout],
    ).unwrap();
}

#[test]
fn extract_command_creates_entities_in_db() {
    let conn = setup();
    insert_git_command(&conn, "cmd-1", "status", " M src/main.rs\n?? new.txt\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    extract::extract_command(&conn, &cmd).unwrap();

    let entities = extract::db::get_entities(&conn, &extract::db::EntityFilter::default()).unwrap();
    assert!(
        entities.iter().any(|e| e.entity_type == "git_file" && e.name == "src/main.rs"),
        "should have git_file entity for src/main.rs, got: {:?}",
        entities
            .iter()
            .map(|e| (&e.entity_type, &e.name))
            .collect::<Vec<_>>()
    );
    assert!(
        entities.iter().any(|e| e.entity_type == "git_repo"),
        "should have git_repo entity"
    );
}

#[test]
fn extract_command_marks_as_extracted() {
    let conn = setup();
    insert_git_command(&conn, "cmd-1", "branch", "* main\n  develop\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    extract::extract_command(&conn, &cmd).unwrap();

    let unextracted = extract::db::get_unextracted_commands(&conn, None, 100).unwrap();
    assert!(
        unextracted.is_empty(),
        "command should be marked as extracted"
    );
}

#[test]
fn extract_command_marks_heuristic_for_git() {
    let conn = setup();
    insert_git_command(&conn, "cmd-1", "status", " M file.rs\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    extract::extract_command(&conn, &cmd).unwrap();

    let method: String = conn
        .query_row(
            "SELECT extraction_method FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(method, "heuristic");
}

#[test]
fn extract_command_marks_generic_for_unknown_domain() {
    let conn = setup();
    insert_generic_command(
        &conn,
        "cmd-1",
        "curl",
        "HTTP/1.1 200 OK\nhttps://api.example.com\n",
    );

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    extract::extract_command(&conn, &cmd).unwrap();

    let method: String = conn
        .query_row(
            "SELECT extraction_method FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(method, "generic");
}

#[test]
fn extract_command_with_no_stdout_marks_skipped() {
    let conn = setup();
    let _ = conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    );
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, source, status)
         VALUES ('cmd-1', 'sess', 1000, 'cd /tmp', 'cd', 'human', 'finished')",
        [],
    )
    .unwrap();

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    extract::extract_command(&conn, &cmd).unwrap();

    let method: Option<String> = conn
        .query_row(
            "SELECT extraction_method FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(method.as_deref(), Some("skipped"));
}

#[test]
fn already_extracted_command_is_skipped() {
    let conn = setup();
    insert_git_command(&conn, "cmd-1", "status", " M file.rs\n");
    conn.execute(
        "UPDATE commands SET extracted = 1, extraction_method = 'heuristic' WHERE id = 'cmd-1'",
        [],
    )
    .unwrap();

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    let result = extract::extract_command(&conn, &cmd);
    assert!(result.is_ok());

    // Should not have created any entities (was already extracted)
    let entities =
        extract::db::get_entities(&conn, &extract::db::EntityFilter::default()).unwrap();
    assert!(entities.is_empty());
}

#[test]
fn extract_creates_relationships() {
    let conn = setup();
    insert_git_command(&conn, "cmd-1", "branch", "* main\n  develop\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    extract::extract_command(&conn, &cmd).unwrap();

    // Branches should have belongs_to relationships to repo
    let entities = extract::db::get_entities(
        &conn,
        &extract::db::EntityFilter {
            entity_type: Some("git_branch"),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!entities.is_empty());
    let rels = extract::db::get_relationships_for(&conn, &entities[0].id).unwrap();
    assert!(
        !rels.is_empty(),
        "branches should have belongs_to relationships"
    );
}
