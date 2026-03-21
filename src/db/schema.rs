use rusqlite::Connection;

pub fn as_json(conn: &Connection) -> serde_json::Value {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap();
    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let tables: Vec<TableInfo> = table_names
        .into_iter()
        .map(|name| {
            let mut stmt2 = conn
                .prepare(&format!("PRAGMA table_info('{}')", name))
                .unwrap();
            let columns = stmt2
                .query_map([], |row| {
                    Ok(ColumnInfo {
                        name: row.get(1)?,
                        col_type: row.get(2)?,
                        notnull: row.get(3)?,
                    })
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            TableInfo { name, columns }
        })
        .collect();

    serde_json::json!({
        "tables": tables.iter().map(table_schema_to_json_schema).collect::<Vec<_>>()
    })
}

struct ColumnInfo {
    name: String,
    col_type: String,
    notnull: bool,
}

struct TableInfo {
    name: String,
    columns: Vec<ColumnInfo>,
}

fn sqlite_type_to_json_type(sqlite_type: &str) -> serde_json::Value {
    let t = sqlite_type.to_lowercase();
    if t.contains("int") {
        serde_json::json!({"type": "integer"})
    } else if t.contains("char") || t.contains("text") {
        serde_json::json!({"type": "string"})
    } else if t.contains("bool") {
        serde_json::json!({"type": "boolean"})
    } else if t.contains("real") || t.contains("floa") || t.contains("doub") {
        serde_json::json!({"type": "number"})
    } else {
        serde_json::json!({"type": "string"})
    }
}

fn table_schema_to_json_schema(table: &TableInfo) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for row in table.columns.iter() {
        properties.insert(row.name.clone(), sqlite_type_to_json_type(&row.col_type));
        if row.notnull {
            required.push(serde_json::json!(row.name));
        }
    }

    let mut schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": table.name,
        "type": "object",
        "properties": properties,
    });

    if !required.is_empty() {
        schema["required"] = serde_json::json!(required);
    }

    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- sqlite_type_to_json_type ---

    #[test]
    fn type_integer_variants() {
        for t in [
            "INTEGER",
            "INT",
            "TINYINT",
            "SMALLINT",
            "MEDIUMINT",
            "BIGINT",
            "INT2",
            "INT8",
        ] {
            assert_eq!(sqlite_type_to_json_type(t)["type"], "integer", "{t}");
        }
    }

    #[test]
    fn type_text_variants() {
        for t in [
            "TEXT",
            "VARCHAR(255)",
            "CHAR(10)",
            "NCHAR(20)",
            "NVARCHAR(50)",
            "CHARACTER(100)",
            "VARYING CHARACTER(200)",
            "CLOB",
        ] {
            assert_eq!(sqlite_type_to_json_type(t)["type"], "string", "{t}");
        }
    }

    #[test]
    fn type_real_variants() {
        for t in ["REAL", "FLOAT", "DOUBLE", "DOUBLE PRECISION"] {
            assert_eq!(sqlite_type_to_json_type(t)["type"], "number", "{t}");
        }
    }

    #[test]
    fn type_boolean() {
        assert_eq!(sqlite_type_to_json_type("BOOLEAN")["type"], "boolean");
    }

    #[test]
    fn type_fallback_string() {
        for t in ["BLOB", "NUMERIC", "DECIMAL", "", "DATE", "DATETIME"] {
            assert_eq!(sqlite_type_to_json_type(t)["type"], "string", "{t}");
        }
    }

    #[test]
    fn type_case_insensitive() {
        assert_eq!(sqlite_type_to_json_type("integer")["type"], "integer");
        assert_eq!(sqlite_type_to_json_type("Integer")["type"], "integer");
        assert_eq!(sqlite_type_to_json_type("real")["type"], "number");
        assert_eq!(sqlite_type_to_json_type("Boolean")["type"], "boolean");
        assert_eq!(sqlite_type_to_json_type("text")["type"], "string");
    }

    fn make_table(name: &str, cols: Vec<(&str, &str, bool)>) -> TableInfo {
        TableInfo {
            name: name.to_string(),
            columns: cols
                .into_iter()
                .map(|(n, t, nn)| ColumnInfo {
                    name: n.to_string(),
                    col_type: t.to_string(),
                    notnull: nn,
                })
                .collect(),
        }
    }

    #[test]
    fn schema_sessions() {
        let t = make_table(
            "sessions",
            vec![
                ("id", "TEXT", true),
                ("name", "TEXT", true),
                ("target", "TEXT", false),
                ("scope", "TEXT", false),
                ("goal", "TEXT", false),
                ("goal_meta", "TEXT", false),
                ("phase", "TEXT", false),
                ("noise_budget", "REAL", false),
                ("autonomy", "TEXT", false),
                ("created_at", "TEXT", false),
                ("updated_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["title"], "sessions");
        assert_eq!(s["properties"]["id"]["type"], "string");
        assert_eq!(s["properties"]["noise_budget"]["type"], "number");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "name"]);
    }

    #[test]
    fn schema_hosts() {
        let t = make_table(
            "hosts",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("ip", "TEXT", true),
                ("hostname", "TEXT", false),
                ("os", "TEXT", false),
                ("status", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["properties"]["id"]["type"], "integer");
        assert_eq!(s["properties"]["ip"]["type"], "string");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "ip"]);
    }

    #[test]
    fn schema_ports() {
        let t = make_table(
            "ports",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("host_id", "INTEGER", true),
                ("port", "INTEGER", true),
                ("protocol", "TEXT", false),
                ("service", "TEXT", false),
                ("version", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["properties"]["port"]["type"], "integer");
        assert_eq!(s["properties"]["host_id"]["type"], "integer");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "host_id", "port"]);
    }

    #[test]
    fn schema_credentials() {
        let t = make_table(
            "credentials",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("username", "TEXT", true),
                ("password", "TEXT", false),
                ("hash", "TEXT", false),
                ("service", "TEXT", false),
                ("host", "TEXT", false),
                ("source", "TEXT", false),
                ("found_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "username"]);
    }

    #[test]
    fn schema_hypotheses() {
        let t = make_table(
            "hypotheses",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("statement", "TEXT", true),
                ("category", "TEXT", true),
                ("status", "TEXT", false),
                ("priority", "TEXT", false),
                ("confidence", "REAL", false),
                ("target_component", "TEXT", false),
                ("created_at", "TEXT", false),
                ("resolved_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["properties"]["confidence"]["type"], "number");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "statement", "category"]);
    }

    #[test]
    fn schema_evidence() {
        let t = make_table(
            "evidence",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("hypothesis_id", "INTEGER", false),
                ("finding", "TEXT", true),
                ("severity", "TEXT", false),
                ("poc", "TEXT", false),
                ("raw_output", "TEXT", false),
                ("created_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["properties"]["hypothesis_id"]["type"], "integer");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "finding"]);
    }

    #[test]
    fn schema_command_history() {
        let t = make_table(
            "command_history",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("command", "TEXT", true),
                ("exit_code", "INTEGER", false),
                ("duration_ms", "INTEGER", false),
                ("output", "TEXT", false),
                ("output_preview", "TEXT", false),
                ("tool", "TEXT", false),
                ("extraction_status", "TEXT", false),
                ("started_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["properties"]["exit_code"]["type"], "integer");
        assert_eq!(s["properties"]["duration_ms"]["type"], "integer");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "command"]);
    }

    #[test]
    fn schema_web_paths() {
        let t = make_table(
            "web_paths",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("host_id", "INTEGER", true),
                ("port", "INTEGER", true),
                ("scheme", "TEXT", true),
                ("path", "TEXT", true),
                ("status_code", "INTEGER", false),
                ("content_length", "INTEGER", false),
                ("content_type", "TEXT", false),
                ("redirect_to", "TEXT", false),
                ("source", "TEXT", false),
                ("found_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["properties"]["status_code"]["type"], "integer");
        assert_eq!(s["properties"]["content_length"]["type"], "integer");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            req,
            vec!["id", "session_id", "host_id", "port", "scheme", "path"]
        );
    }

    #[test]
    fn schema_vulns() {
        let t = make_table(
            "vulns",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("host_id", "INTEGER", true),
                ("port", "INTEGER", true),
                ("name", "TEXT", true),
                ("severity", "TEXT", false),
                ("cve", "TEXT", false),
                ("url", "TEXT", false),
                ("detail", "TEXT", false),
                ("source", "TEXT", false),
                ("found_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "host_id", "port", "name"]);
    }

    #[test]
    fn schema_chat_messages() {
        let t = make_table(
            "chat_messages",
            vec![
                ("id", "INTEGER", true),
                ("session_id", "TEXT", true),
                ("role", "TEXT", true),
                ("content", "TEXT", true),
                ("created_at", "TEXT", false),
            ],
        );
        let s = table_schema_to_json_schema(&t);
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(req, vec!["id", "session_id", "role", "content"]);
    }

    #[test]
    fn schema_empty_table() {
        let t = make_table("empty", vec![]);
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["title"], "empty");
        assert!(s["properties"].as_object().unwrap().is_empty());
        assert!(s.get("required").is_none());
    }

    #[test]
    fn schema_no_required_when_all_nullable() {
        let t = make_table("loose", vec![("a", "TEXT", false), ("b", "INTEGER", false)]);
        let s = table_schema_to_json_schema(&t);
        assert!(s.get("required").is_none());
    }

    #[test]
    fn schema_has_json_schema_meta() {
        let t = make_table("any", vec![("x", "TEXT", false)]);
        let s = table_schema_to_json_schema(&t);
        assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(s["type"], "object");
    }
}
