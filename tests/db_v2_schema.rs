use redtrail::db_v2::DbV2;

#[test]
fn creates_all_tables() {
    let db = DbV2::open_in_memory().unwrap();
    let tables: Vec<String> = db.conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let expected = vec![
        "access_levels", "attack_paths", "attack_patterns",
        "command_history", "credentials", "failed_attempts",
        "findings", "flags", "hosts", "ports", "sessions",
        "technique_executions",
    ];
    for table in &expected {
        assert!(tables.contains(&table.to_string()), "missing table: {table}");
    }
}

#[test]
fn sessions_table_has_expected_columns() {
    let db = DbV2::open_in_memory().unwrap();
    let cols: Vec<String> = db.conn()
        .prepare("PRAGMA table_info(sessions)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for col in &["id", "name", "status", "env_json", "tool_config_json",
                  "llm_provider", "llm_model", "working_dir", "prompt_template"] {
        assert!(cols.contains(&col.to_string()), "missing column: {col}");
    }
}
