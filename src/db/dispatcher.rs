use std::collections::HashMap;
use rusqlite::{Connection, params_from_iter};
use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct CreateResult {
    pub id: i64,
    pub created: bool,
}

struct TableDef {
    columns: &'static [&'static str],
    unique_key: &'static [&'static str],
    validators: &'static [(&'static str, fn(&serde_json::Value) -> bool)],
}

static TABLES: &[(&str, TableDef)] = &[
    ("hosts", TableDef {
        columns: &["ip", "hostname", "os", "status"],
        unique_key: &["session_id", "ip"],
        validators: &[("status", valid_host_status)],
    }),
    ("ports", TableDef {
        columns: &["host_id", "port", "protocol", "service", "version"],
        unique_key: &["host_id", "port", "protocol"],
        validators: &[("protocol", valid_protocol), ("port", valid_port_range)],
    }),
    ("credentials", TableDef {
        columns: &["username", "password", "hash", "service", "host", "source"],
        unique_key: &[],
        validators: &[],
    }),
    ("access_levels", TableDef {
        columns: &["host", "user", "level", "method"],
        unique_key: &[],
        validators: &[("level", valid_access_level)],
    }),
    ("flags", TableDef {
        columns: &["value", "source"],
        unique_key: &[],
        validators: &[],
    }),
    ("notes", TableDef {
        columns: &["text"],
        unique_key: &[],
        validators: &[],
    }),
    ("web_paths", TableDef {
        columns: &["host_id", "port", "scheme", "path", "status_code", "content_length", "content_type", "redirect_to", "source"],
        unique_key: &["session_id", "host_id", "port", "path"],
        validators: &[("scheme", valid_scheme), ("status_code", valid_status_code)],
    }),
    ("vulns", TableDef {
        columns: &["host_id", "port", "name", "severity", "cve", "url", "detail", "source"],
        unique_key: &["session_id", "host_id", "port", "name"],
        validators: &[("severity", valid_severity)],
    }),
    ("hypotheses", TableDef {
        columns: &["statement", "category", "status", "priority", "confidence", "target_component"],
        unique_key: &[],
        validators: &[("status", valid_hypothesis_status), ("priority", valid_priority), ("confidence", valid_confidence)],
    }),
    ("evidence", TableDef {
        columns: &["hypothesis_id", "finding", "severity", "poc", "raw_output"],
        unique_key: &[],
        validators: &[("severity", valid_evidence_severity)],
    }),
];

const PROTECTED_TABLES: &[&str] = &["sessions", "command_history", "chat_messages"];

fn lookup_table(name: &str) -> Result<&'static TableDef, Error> {
    if PROTECTED_TABLES.contains(&name) {
        return Err(Error::Db(format!("table '{name}' is protected and cannot be written via dispatcher")));
    }
    TABLES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, def)| def)
        .ok_or_else(|| Error::Db(format!("unknown table '{name}'")))
}

pub fn create(
    conn: &Connection,
    session_id: &str,
    table: &str,
    data: &HashMap<String, serde_json::Value>,
) -> Result<CreateResult, Error> {
    let def = lookup_table(table)?;

    if data.contains_key("session_id") {
        return Err(Error::Db("session_id is auto-injected; do not provide it".into()));
    }

    for key in data.keys() {
        if !def.columns.contains(&key.as_str()) {
            return Err(Error::Db(format!(
                "column '{key}' not allowed for table '{table}'; allowed: {:?}",
                def.columns
            )));
        }
    }

    for &(col, validator) in def.validators {
        if let Some(val) = data.get(col) {
            if !val.is_null() && !validator(val) {
                return Err(Error::Db(format!(
                    "invalid value for '{table}.{col}': {val}"
                )));
            }
        }
    }

    let mut col_names = vec!["session_id".to_string()];
    let mut placeholders = vec!["?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(session_id.to_string())];
    let mut idx = 2;

    for col in def.columns {
        if let Some(val) = data.get(*col) {
            col_names.push(col.to_string());
            placeholders.push(format!("?{idx}"));
            values.push(json_to_sql(val));
            idx += 1;
        }
    }

    let cols_str = col_names.join(", ");
    let placeholders_str = placeholders.join(", ");
    let sql = format!(
        "INSERT OR IGNORE INTO {table} ({cols_str}) VALUES ({placeholders_str})"
    );

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let rows_changed = conn
        .execute(&sql, params_from_iter(params.iter()))
        .map_err(|e| Error::Db(e.to_string()))?;

    let created = rows_changed > 0;

    let id = if created {
        conn.last_insert_rowid()
    } else if !def.unique_key.is_empty() {
        lookup_existing_id(conn, table, session_id, def, data)?
    } else {
        return Err(Error::Db(format!(
            "duplicate row in '{table}' but no unique key to look up existing id"
        )));
    };

    Ok(CreateResult { id, created })
}

fn lookup_existing_id(
    conn: &Connection,
    table: &str,
    session_id: &str,
    def: &TableDef,
    data: &HashMap<String, serde_json::Value>,
) -> Result<i64, Error> {
    let mut conditions = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    for &col in def.unique_key {
        conditions.push(format!("{col} = ?{idx}"));
        if col == "session_id" {
            values.push(Box::new(session_id.to_string()));
        } else {
            let val = data.get(col).ok_or_else(|| {
                Error::Db(format!("unique key column '{col}' required for dedup lookup"))
            })?;
            values.push(json_to_sql(val));
        }
        idx += 1;
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!("SELECT id FROM {table} WHERE {where_clause}");
    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();

    conn.query_row(&sql, params_from_iter(params.iter()), |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))
}

fn json_to_sql(val: &serde_json::Value) -> Box<dyn rusqlite::types::ToSql> {
    match val {
        serde_json::Value::Null => Box::new(Option::<String>::None),
        serde_json::Value::Bool(b) => Box::new(*b as i64),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        serde_json::Value::String(s) => Box::new(s.clone()),
        _ => Box::new(val.to_string()),
    }
}

fn valid_host_status(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("up" | "down" | "unknown"))
}

fn valid_protocol(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("tcp" | "udp" | "sctp"))
}

fn valid_port_range(v: &serde_json::Value) -> bool {
    v.as_i64().is_some_and(|n| (0..=65535).contains(&n))
}

fn valid_access_level(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("user" | "root" | "admin" | "system" | "service"))
}

fn valid_scheme(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("http" | "https"))
}

fn valid_status_code(v: &serde_json::Value) -> bool {
    v.as_i64().is_some_and(|n| (100..=599).contains(&n))
}

fn valid_severity(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("info" | "low" | "medium" | "high" | "critical"))
}

fn valid_hypothesis_status(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("pending" | "testing" | "confirmed" | "refuted"))
}

fn valid_priority(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("low" | "medium" | "high" | "critical"))
}

fn valid_confidence(v: &serde_json::Value) -> bool {
    v.as_f64().is_some_and(|n| (0.0..=1.0).contains(&n))
}

fn valid_evidence_severity(v: &serde_json::Value) -> bool {
    matches!(v.as_str(), Some("info" | "low" | "medium" | "high" | "critical"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name) VALUES ('s1', 'test')",
            [],
        ).unwrap();
        conn
    }

    fn map(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn create_host_new() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.1")), ("os", serde_json::json!("Linux"))]);
        let r = create(&conn, "s1", "hosts", &data).unwrap();
        assert!(r.created);
        assert!(r.id > 0);
    }

    #[test]
    fn create_host_duplicate_returns_existing_id() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.1"))]);
        let r1 = create(&conn, "s1", "hosts", &data).unwrap();
        let r2 = create(&conn, "s1", "hosts", &data).unwrap();
        assert!(r1.created);
        assert!(!r2.created);
        assert_eq!(r1.id, r2.id);
    }

    #[test]
    fn create_rejects_unknown_table() {
        let conn = setup();
        let data = map(&[]);
        let err = create(&conn, "s1", "nonexistent", &data).unwrap_err();
        assert!(err.to_string().contains("unknown table"));
    }

    #[test]
    fn create_rejects_protected_table() {
        let conn = setup();
        let data = map(&[]);
        for t in &["sessions", "command_history", "chat_messages"] {
            let err = create(&conn, "s1", t, &data).unwrap_err();
            assert!(err.to_string().contains("protected"), "table {t}");
        }
    }

    #[test]
    fn create_rejects_unknown_column() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.1")), ("bogus", serde_json::json!("x"))]);
        let err = create(&conn, "s1", "hosts", &data).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn create_rejects_caller_session_id() {
        let conn = setup();
        let data = map(&[("session_id", serde_json::json!("s1")), ("ip", serde_json::json!("10.10.10.1"))]);
        let err = create(&conn, "s1", "hosts", &data).unwrap_err();
        assert!(err.to_string().contains("auto-injected"));
    }

    #[test]
    fn create_validates_enum_host_status() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.1")), ("status", serde_json::json!("maybe"))]);
        let err = create(&conn, "s1", "hosts", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_port_range() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[("host_id", serde_json::json!(1)), ("port", serde_json::json!(99999)), ("protocol", serde_json::json!("tcp"))]);
        let err = create(&conn, "s1", "ports", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_protocol() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[("host_id", serde_json::json!(1)), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("icmp"))]);
        let err = create(&conn, "s1", "ports", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_severity() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[
            ("host_id", serde_json::json!(1)),
            ("port", serde_json::json!(80)),
            ("name", serde_json::json!("XSS")),
            ("severity", serde_json::json!("extreme")),
        ]);
        let err = create(&conn, "s1", "vulns", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_confidence() {
        let conn = setup();
        let data = map(&[
            ("statement", serde_json::json!("test")),
            ("category", serde_json::json!("auth")),
            ("confidence", serde_json::json!(1.5)),
        ]);
        let err = create(&conn, "s1", "hypotheses", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_all_10_tables() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let host_id: i64 = conn.query_row("SELECT id FROM hosts WHERE ip='10.10.10.1'", [], |r| r.get(0)).unwrap();

        let tables_data: Vec<(&str, HashMap<String, serde_json::Value>)> = vec![
            ("hosts", map(&[("ip", serde_json::json!("10.10.10.2"))])),
            ("ports", map(&[("host_id", serde_json::json!(host_id)), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))])),
            ("credentials", map(&[("username", serde_json::json!("admin")), ("password", serde_json::json!("secret"))])),
            ("access_levels", map(&[("host", serde_json::json!("10.10.10.1")), ("user", serde_json::json!("root")), ("level", serde_json::json!("root"))])),
            ("flags", map(&[("value", serde_json::json!("HTB{test}"))])),
            ("notes", map(&[("text", serde_json::json!("found something"))])),
            ("web_paths", map(&[("host_id", serde_json::json!(host_id)), ("port", serde_json::json!(80)), ("scheme", serde_json::json!("http")), ("path", serde_json::json!("/admin"))])),
            ("vulns", map(&[("host_id", serde_json::json!(host_id)), ("port", serde_json::json!(80)), ("name", serde_json::json!("XSS")), ("severity", serde_json::json!("medium"))])),
            ("hypotheses", map(&[("statement", serde_json::json!("SSH weak")), ("category", serde_json::json!("auth"))])),
            ("evidence", map(&[("finding", serde_json::json!("found vuln")), ("severity", serde_json::json!("high"))])),
        ];

        for (table, data) in tables_data {
            let r = create(&conn, "s1", table, &data).unwrap();
            assert!(r.created, "table {table} should create");
            assert!(r.id > 0, "table {table} should return positive id");
        }
    }

    #[test]
    fn create_null_value_skips_validator() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.1")), ("status", serde_json::Value::Null)]);
        let r = create(&conn, "s1", "hosts", &data).unwrap();
        assert!(r.created);
    }

    #[test]
    fn create_port_duplicate_returns_existing() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[("host_id", serde_json::json!(1)), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))]);
        let r1 = create(&conn, "s1", "ports", &data).unwrap();
        let r2 = create(&conn, "s1", "ports", &data).unwrap();
        assert!(r1.created);
        assert!(!r2.created);
        assert_eq!(r1.id, r2.id);
    }

    #[test]
    fn create_web_path_duplicate() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[
            ("host_id", serde_json::json!(1)),
            ("port", serde_json::json!(80)),
            ("scheme", serde_json::json!("http")),
            ("path", serde_json::json!("/admin")),
        ]);
        let r1 = create(&conn, "s1", "web_paths", &data).unwrap();
        let r2 = create(&conn, "s1", "web_paths", &data).unwrap();
        assert!(r1.created);
        assert!(!r2.created);
        assert_eq!(r1.id, r2.id);
    }

    #[test]
    fn create_vuln_duplicate() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[
            ("host_id", serde_json::json!(1)),
            ("port", serde_json::json!(80)),
            ("name", serde_json::json!("XSS")),
            ("severity", serde_json::json!("medium")),
        ]);
        let r1 = create(&conn, "s1", "vulns", &data).unwrap();
        let r2 = create(&conn, "s1", "vulns", &data).unwrap();
        assert!(r1.created);
        assert!(!r2.created);
        assert_eq!(r1.id, r2.id);
    }

    #[test]
    fn create_validates_access_level_enum() {
        let conn = setup();
        let data = map(&[
            ("host", serde_json::json!("10.10.10.1")),
            ("user", serde_json::json!("bob")),
            ("level", serde_json::json!("superuser")),
        ]);
        let err = create(&conn, "s1", "access_levels", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_scheme_enum() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[
            ("host_id", serde_json::json!(1)),
            ("port", serde_json::json!(80)),
            ("scheme", serde_json::json!("ftp")),
            ("path", serde_json::json!("/admin")),
        ]);
        let err = create(&conn, "s1", "web_paths", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_hypothesis_status_enum() {
        let conn = setup();
        let data = map(&[
            ("statement", serde_json::json!("test")),
            ("category", serde_json::json!("auth")),
            ("status", serde_json::json!("maybe")),
        ]);
        let err = create(&conn, "s1", "hypotheses", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn create_validates_status_code_range() {
        let conn = setup();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        ).unwrap();
        let data = map(&[
            ("host_id", serde_json::json!(1)),
            ("port", serde_json::json!(80)),
            ("scheme", serde_json::json!("http")),
            ("path", serde_json::json!("/x")),
            ("status_code", serde_json::json!(999)),
        ]);
        let err = create(&conn, "s1", "web_paths", &data).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }
}
