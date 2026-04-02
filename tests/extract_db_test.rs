use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
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
