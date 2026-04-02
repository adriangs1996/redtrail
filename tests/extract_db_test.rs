use redtrail::core::db;
use redtrail::extract::db as extract_db;
use redtrail::extract::types::{NewEntity, NewRelationship};

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn insert_test_command(conn: &rusqlite::Connection, id: &str) {
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, source, status) VALUES (?1, 'sess', 1000, 'test cmd', 'human', 'finished')",
        [id],
    ).unwrap();
}

#[test]
fn schema_has_extraction_method_on_commands() {
    let conn = setup();
    let has_col: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .any(|col| col.as_deref() == Ok("extraction_method"));
    assert!(has_col, "commands table missing extraction_method column");
}

#[test]
fn schema_has_canonical_key_on_entities() {
    let conn = setup();
    let has_col: bool = conn
        .prepare("PRAGMA table_info(entities)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .any(|col| col.as_deref() == Ok("canonical_key"));
    assert!(has_col, "entities table missing canonical_key column");
}

#[test]
fn schema_has_entity_observations_table() {
    let conn = setup();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entity_observations'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "entity_observations table missing");
}

#[test]
fn schema_has_git_typed_tables() {
    let conn = setup();
    for table in &[
        "git_branches",
        "git_commits",
        "git_remotes",
        "git_files",
        "git_tags",
        "git_stashes",
    ] {
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing table: {table}");
    }
}

#[test]
fn schema_has_docker_typed_tables() {
    let conn = setup();
    for table in &[
        "docker_containers",
        "docker_images",
        "docker_networks",
        "docker_volumes",
        "docker_services",
    ] {
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "missing table: {table}");
    }
}

#[test]
fn entities_canonical_key_unique_constraint() {
    let conn = setup();
    conn.execute(
        "INSERT INTO entities (id, type, name, canonical_key, first_seen, last_seen) VALUES ('e1', 'file', 'foo.rs', '/src/foo.rs', 1000, 1000)",
        [],
    ).unwrap();
    let result = conn.execute(
        "INSERT INTO entities (id, type, name, canonical_key, first_seen, last_seen) VALUES ('e2', 'file', 'foo.rs', '/src/foo.rs', 2000, 2000)",
        [],
    );
    assert!(
        result.is_err(),
        "UNIQUE(type, canonical_key) constraint should prevent duplicate"
    );
}

#[test]
fn upsert_entity_creates_new() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");
    let entity = NewEntity {
        entity_type: "file".into(),
        name: "main.rs".into(),
        canonical_key: "/src/main.rs".into(),
        properties: Some(serde_json::json!({"status": "modified"})),
        typed_data: None,
        observation_context: Some("modified".into()),
    };
    let id = extract_db::upsert_entity(&conn, &entity, "cmd-1", 1000).unwrap();
    assert!(!id.is_empty());

    let row = extract_db::get_entity(&conn, &id).unwrap();
    assert_eq!(row.entity_type, "file");
    assert_eq!(row.name, "main.rs");
    assert_eq!(row.canonical_key, "/src/main.rs");
}

#[test]
fn upsert_entity_merges_on_conflict() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");
    insert_test_command(&conn, "cmd-2");

    let e1 = NewEntity {
        entity_type: "file".into(),
        name: "main.rs".into(),
        canonical_key: "/src/main.rs".into(),
        properties: Some(serde_json::json!({"status": "modified"})),
        typed_data: None,
        observation_context: None,
    };
    let id1 = extract_db::upsert_entity(&conn, &e1, "cmd-1", 1000).unwrap();

    let e2 = NewEntity {
        entity_type: "file".into(),
        name: "main.rs".into(),
        canonical_key: "/src/main.rs".into(),
        properties: Some(serde_json::json!({"status": "staged", "lines": 42})),
        typed_data: None,
        observation_context: None,
    };
    let id2 = extract_db::upsert_entity(&conn, &e2, "cmd-2", 2000).unwrap();
    assert_eq!(id1, id2, "same entity should return same ID on upsert");

    let row = extract_db::get_entity(&conn, &id1).unwrap();
    assert_eq!(row.last_seen, 2000);
}

#[test]
fn insert_observation_on_upsert() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");
    insert_test_command(&conn, "cmd-2");

    let entity = NewEntity {
        entity_type: "file".into(),
        name: "lib.rs".into(),
        canonical_key: "/src/lib.rs".into(),
        properties: None,
        typed_data: None,
        observation_context: Some("read".into()),
    };
    extract_db::upsert_entity(&conn, &entity, "cmd-1", 1000).unwrap();
    extract_db::upsert_entity(&conn, &entity, "cmd-2", 2000).unwrap();

    let obs = extract_db::get_entity_observations_by_key(&conn, "file", "/src/lib.rs").unwrap();
    assert_eq!(obs.len(), 2);
}

#[test]
fn insert_relationship_resolves_entities() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");

    let file = NewEntity {
        entity_type: "git_file".into(),
        name: "main.rs".into(),
        canonical_key: "repo:/src/main.rs".into(),
        properties: None,
        typed_data: None,
        observation_context: None,
    };
    let repo = NewEntity {
        entity_type: "git_repo".into(),
        name: "redtrail".into(),
        canonical_key: "/home/user/redtrail".into(),
        properties: None,
        typed_data: None,
        observation_context: None,
    };
    extract_db::upsert_entity(&conn, &file, "cmd-1", 1000).unwrap();
    extract_db::upsert_entity(&conn, &repo, "cmd-1", 1000).unwrap();

    let rel = NewRelationship {
        source_type: "git_file".into(),
        source_canonical_key: "repo:/src/main.rs".into(),
        target_type: "git_repo".into(),
        target_canonical_key: "/home/user/redtrail".into(),
        relation_type: "belongs_to".into(),
        properties: None,
    };
    let result = extract_db::insert_relationship(&conn, &rel, "cmd-1", 1000);
    assert!(result.is_ok());

    // Verify relationship is retrievable
    let file_id = extract_db::get_entities(
        &conn,
        &extract_db::EntityFilter {
            entity_type: Some("git_file"),
            ..Default::default()
        },
    )
    .unwrap()[0]
        .id
        .clone();
    let rels = extract_db::get_relationships_for(&conn, &file_id).unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].relation_type, "belongs_to");
}

#[test]
fn get_unextracted_commands_filters_correctly() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");
    insert_test_command(&conn, "cmd-2");
    extract_db::mark_extracted(&conn, "cmd-1", "heuristic").unwrap();

    let unextracted = extract_db::get_unextracted_commands(&conn, None, 100).unwrap();
    assert_eq!(unextracted.len(), 1);
    assert_eq!(unextracted[0].id, "cmd-2");
}

#[test]
fn get_entities_filtered_by_type() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");

    let e1 = NewEntity {
        entity_type: "file".into(),
        name: "a.rs".into(),
        canonical_key: "/a.rs".into(),
        properties: None,
        typed_data: None,
        observation_context: None,
    };
    let e2 = NewEntity {
        entity_type: "git_branch".into(),
        name: "main".into(),
        canonical_key: "repo:main:false".into(),
        properties: None,
        typed_data: None,
        observation_context: None,
    };
    extract_db::upsert_entity(&conn, &e1, "cmd-1", 1000).unwrap();
    extract_db::upsert_entity(&conn, &e2, "cmd-1", 1000).unwrap();

    let files = extract_db::get_entities(
        &conn,
        &extract_db::EntityFilter {
            entity_type: Some("file"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "a.rs");
}

#[test]
fn mark_extracted_sets_flag_and_method() {
    let conn = setup();
    insert_test_command(&conn, "cmd-1");
    extract_db::mark_extracted(&conn, "cmd-1", "heuristic").unwrap();

    let extracted: bool = conn
        .query_row(
            "SELECT extracted FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(extracted);

    let method: String = conn
        .query_row(
            "SELECT extraction_method FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(method, "heuristic");
}
