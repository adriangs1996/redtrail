use redtrail::core::fmt;

#[test]
fn format_table_basic() {
    let cols = vec!["name".into(), "value".into()];
    let rows = vec![
        vec![serde_json::json!("host"), serde_json::json!("10.10.10.1")],
        vec![serde_json::json!("port"), serde_json::json!(22)],
    ];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("name"));
    assert!(out.contains("value"));
    assert!(out.contains("10.10.10.1"));
    assert!(out.contains("22"));
    assert!(out.contains("(2 rows)"));
}

#[test]
fn format_table_empty() {
    let cols: Vec<String> = vec![];
    let rows: Vec<Vec<serde_json::Value>> = vec![];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("(0 rows)"));
}

#[test]
fn format_table_null_values() {
    let cols = vec!["col".into()];
    let rows = vec![vec![serde_json::Value::Null]];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("NULL"));
}

#[test]
fn format_table_column_padding() {
    let cols = vec!["short".into(), "long_column_name".into()];
    let rows = vec![
        vec![serde_json::json!("a"), serde_json::json!("b")],
    ];
    let out = fmt::format("table", &cols, &rows);
    // Separator should be at least as wide as column names
    assert!(out.contains("long_column_name"));
    let lines: Vec<&str> = out.lines().collect();
    // Header and separator should have the same width
    assert_eq!(lines[0].trim_end().len(), lines[1].trim_end().len());
}

#[test]
fn format_table_json_objects() {
    let cols = vec!["data".into()];
    let rows = vec![vec![serde_json::json!({"nested": true})]];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("nested"));
}
