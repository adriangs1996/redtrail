use rusqlite::Connection;

pub(crate) const PROTECTED_TABLES: &[&str] = &["sessions", "command_history", "chat_messages"];

struct Constraint {
    table: &'static str,
    column: &'static str,
    kind: ConstraintKind,
}

enum ConstraintKind {
    Enum(&'static [&'static str]),
    Range(f64, f64),
}

static CONSTRAINTS: &[Constraint] = &[
    Constraint { table: "hosts", column: "status", kind: ConstraintKind::Enum(&["up", "down", "unknown"]) },
    Constraint { table: "ports", column: "protocol", kind: ConstraintKind::Enum(&["tcp", "udp", "sctp"]) },
    Constraint { table: "ports", column: "port", kind: ConstraintKind::Range(1.0, 65535.0) },
    Constraint { table: "web_paths", column: "scheme", kind: ConstraintKind::Enum(&["http", "https"]) },
    Constraint { table: "web_paths", column: "port", kind: ConstraintKind::Range(1.0, 65535.0) },
    Constraint { table: "web_paths", column: "status_code", kind: ConstraintKind::Range(100.0, 599.0) },
    Constraint { table: "vulns", column: "severity", kind: ConstraintKind::Enum(&["info", "low", "medium", "high", "critical"]) },
    Constraint { table: "hypotheses", column: "status", kind: ConstraintKind::Enum(&["pending", "testing", "confirmed", "refuted"]) },
    Constraint { table: "hypotheses", column: "priority", kind: ConstraintKind::Enum(&["low", "medium", "high", "critical"]) },
    Constraint { table: "hypotheses", column: "confidence", kind: ConstraintKind::Range(0.0, 1.0) },
    Constraint { table: "evidence", column: "severity", kind: ConstraintKind::Enum(&["info", "low", "medium", "high", "critical"]) },
];

#[allow(dead_code)] // Used in tests; will be wired into schema export command
pub fn as_json(conn: &Connection) -> serde_json::Value {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap();
    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|name: &String| !PROTECTED_TABLES.contains(&name.as_str()))
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

fn apply_constraints(table_name: &str, col_name: &str, mut prop: serde_json::Value) -> serde_json::Value {
    for c in CONSTRAINTS {
        if c.table == table_name && c.column == col_name {
            match &c.kind {
                ConstraintKind::Enum(values) => {
                    prop["enum"] = serde_json::json!(values);
                }
                ConstraintKind::Range(min, max) => {
                    prop["minimum"] = serde_json::json!(min);
                    prop["maximum"] = serde_json::json!(max);
                }
            }
            break;
        }
    }
    prop
}

fn table_schema_to_json_schema(table: &TableInfo) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for row in table.columns.iter() {
        let prop = sqlite_type_to_json_type(&row.col_type);
        let prop = apply_constraints(&table.name, &row.name, prop);
        properties.insert(row.name.clone(), prop);
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
        let status_enum: Vec<&str> = s["properties"]["status"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(status_enum, vec!["up", "down", "unknown"]);
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
        assert_eq!(s["properties"]["port"]["minimum"], 1.0);
        assert_eq!(s["properties"]["port"]["maximum"], 65535.0);
        let proto_enum: Vec<&str> = s["properties"]["protocol"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(proto_enum, vec!["tcp", "udp", "sctp"]);
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
        assert_eq!(s["properties"]["confidence"]["minimum"], 0.0);
        assert_eq!(s["properties"]["confidence"]["maximum"], 1.0);
        let status_enum: Vec<&str> = s["properties"]["status"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(status_enum, vec!["pending", "testing", "confirmed", "refuted"]);
        let prio_enum: Vec<&str> = s["properties"]["priority"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(prio_enum, vec!["low", "medium", "high", "critical"]);
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
        let sev_enum: Vec<&str> = s["properties"]["severity"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(sev_enum, vec!["info", "low", "medium", "high", "critical"]);
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
        assert_eq!(s["properties"]["port"]["minimum"], 1.0);
        assert_eq!(s["properties"]["port"]["maximum"], 65535.0);
        assert_eq!(s["properties"]["status_code"]["minimum"], 100.0);
        assert_eq!(s["properties"]["status_code"]["maximum"], 599.0);
        let scheme_enum: Vec<&str> = s["properties"]["scheme"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(scheme_enum, vec!["http", "https"]);
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
        let sev_enum: Vec<&str> = s["properties"]["severity"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(sev_enum, vec!["info", "low", "medium", "high", "critical"]);
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

    #[test]
    fn as_json_excludes_protected_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE command_history (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, command TEXT NOT NULL);
            CREATE TABLE chat_messages (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL);
            CREATE TABLE hosts (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, ip TEXT NOT NULL);
            CREATE TABLE ports (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, host_id INTEGER NOT NULL, port INTEGER NOT NULL);
        ").unwrap();
        let result = as_json(&conn);
        let tables = result["tables"].as_array().unwrap();
        let names: Vec<&str> = tables.iter().map(|t| t["title"].as_str().unwrap()).collect();
        assert!(!names.contains(&"sessions"));
        assert!(!names.contains(&"command_history"));
        assert!(!names.contains(&"chat_messages"));
        assert!(names.contains(&"hosts"));
        assert!(names.contains(&"ports"));
    }

    #[test]
    fn as_json_includes_constraints() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE hosts (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, ip TEXT NOT NULL, status TEXT);
            CREATE TABLE ports (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, host_id INTEGER NOT NULL, port INTEGER NOT NULL, protocol TEXT);
        ").unwrap();
        let result = as_json(&conn);
        let tables = result["tables"].as_array().unwrap();
        let hosts = tables.iter().find(|t| t["title"] == "hosts").unwrap();
        assert!(hosts["properties"]["status"]["enum"].as_array().is_some());
        let ports = tables.iter().find(|t| t["title"] == "ports").unwrap();
        assert!(ports["properties"]["protocol"]["enum"].as_array().is_some());
        assert!(ports["properties"]["port"]["minimum"].as_f64().is_some());
    }

    #[test]
    fn no_constraints_on_unconstrained_column() {
        let t = make_table("hosts", vec![("ip", "TEXT", true)]);
        let s = table_schema_to_json_schema(&t);
        assert!(s["properties"]["ip"].get("enum").is_none());
        assert!(s["properties"]["ip"].get("minimum").is_none());
    }
}
