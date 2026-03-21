use std::collections::HashMap;
use rusqlite::{Connection, OptionalExtension, params_from_iter};
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
        columns: &["host_id", "ip", "port", "protocol", "service", "version"],
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
        columns: &["host_id", "ip", "port", "scheme", "path", "status_code", "content_length", "content_type", "redirect_to", "source"],
        unique_key: &["session_id", "host_id", "port", "path"],
        validators: &[("scheme", valid_scheme), ("status_code", valid_status_code)],
    }),
    ("vulns", TableDef {
        columns: &["host_id", "ip", "port", "name", "severity", "cve", "url", "detail", "source"],
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

const IP_RESOLVABLE_TABLES: &[&str] = &["ports", "web_paths", "vulns"];

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

    let mut data = data.clone();

    if IP_RESOLVABLE_TABLES.contains(&table) {
        if let Some(ip_val) = data.remove("ip") {
            if data.contains_key("host_id") {
                return Err(Error::Db("provide either 'ip' or 'host_id', not both".into()));
            }
            let ip = ip_val.as_str().ok_or_else(|| Error::Db("ip must be a string".into()))?;
            let host_id = resolve_host_id(conn, session_id, ip)?;
            data.insert("host_id".to_string(), serde_json::json!(host_id));
        }
    }

    if table == "evidence" {
        if let Some(hid_val) = data.get("hypothesis_id") {
            if !hid_val.is_null() {
                let hid = hid_val.as_i64().ok_or_else(|| Error::Db("hypothesis_id must be an integer".into()))?;
                validate_hypothesis_session(conn, session_id, hid)?;
            }
        }
    }

    let mut col_names = vec!["session_id".to_string()];
    let mut placeholders = vec!["?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(session_id.to_string())];
    let mut idx = 2;

    for col in def.columns {
        if *col == "ip" && IP_RESOLVABLE_TABLES.contains(&table) { continue; }
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
        lookup_existing_id(conn, table, session_id, def, &data)?
    } else {
        return Err(Error::Db(format!(
            "duplicate row in '{table}' but no unique key to look up existing id"
        )));
    };

    Ok(CreateResult { id, created })
}

pub fn query(
    conn: &Connection,
    session_id: &str,
    table: &str,
    filters: &HashMap<String, serde_json::Value>,
) -> Result<Vec<serde_json::Value>, Error> {
    let def = lookup_table(table)?;

    let allowed_filter_keys: Vec<&str> = std::iter::once("id")
        .chain(def.columns.iter().copied())
        .collect();

    let has_ip_filter = filters.contains_key("ip") && IP_RESOLVABLE_TABLES.contains(&table);

    for key in filters.keys() {
        if key == "ip" && IP_RESOLVABLE_TABLES.contains(&table) {
            continue;
        }
        if !allowed_filter_keys.contains(&key.as_str()) {
            return Err(Error::Db(format!(
                "filter key '{key}' not allowed for table '{table}'; allowed: {:?}",
                allowed_filter_keys
            )));
        }
    }

    let col_names = get_all_columns(conn, table)?;

    let mut sql = if has_ip_filter {
        format!(
            "SELECT {cols} FROM {table} INNER JOIN hosts ON {table}.host_id = hosts.id AND hosts.session_id = {table}.session_id WHERE {table}.session_id = ?1",
            cols = col_names.iter().map(|c| format!("{table}.{c}")).collect::<Vec<_>>().join(", "),
        )
    } else {
        format!(
            "SELECT {cols} FROM {table} WHERE {table}.session_id = ?1",
            cols = col_names.join(", "),
        )
    };

    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(session_id.to_string())];
    let mut idx = 2;

    for (key, val) in filters {
        if key == "ip" && has_ip_filter {
            sql.push_str(&format!(" AND hosts.ip = ?{idx}"));
            values.push(json_to_sql(val));
            idx += 1;
        } else {
            sql.push_str(&format!(" AND {table}.{key} = ?{idx}"));
            values.push(json_to_sql(val));
            idx += 1;
        }
    }

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            let mut obj = serde_json::Map::new();
            for (i, col) in col_names.iter().enumerate() {
                let val: rusqlite::types::Value = row.get(i)?;
                obj.insert(col.clone(), sqlite_to_json(val));
            }
            Ok(serde_json::Value::Object(obj))
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

fn get_all_columns(conn: &Connection, table: &str) -> Result<Vec<String>, Error> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| Error::Db(e.to_string()))?;
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    if cols.is_empty() {
        return Err(Error::Db(format!("no columns found for table '{table}'")));
    }
    Ok(cols)
}

fn sqlite_to_json(val: rusqlite::types::Value) -> serde_json::Value {
    match val {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(i) => serde_json::json!(i),
        rusqlite::types::Value::Real(f) => serde_json::json!(f),
        rusqlite::types::Value::Text(s) => serde_json::json!(s),
        rusqlite::types::Value::Blob(b) => serde_json::json!(format!("<blob:{} bytes>", b.len())),
    }
}

fn resolve_host_id(conn: &Connection, session_id: &str, ip: &str) -> Result<i64, Error> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM hosts WHERE session_id = ?1 AND ip = ?2",
            rusqlite::params![session_id, ip],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| Error::Db(e.to_string()))?;

    if let Some(id) = existing {
        return Ok(id);
    }

    conn.execute(
        "INSERT INTO hosts (session_id, ip) VALUES (?1, ?2)",
        rusqlite::params![session_id, ip],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

fn validate_hypothesis_session(conn: &Connection, session_id: &str, hypothesis_id: i64) -> Result<(), Error> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM hypotheses WHERE id = ?1 AND session_id = ?2)",
            rusqlite::params![hypothesis_id, session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    if !exists {
        return Err(Error::Db(format!(
            "hypothesis_id {hypothesis_id} does not belong to session '{session_id}'"
        )));
    }
    Ok(())
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

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateResult {
    pub updated: bool,
}

pub fn update(
    conn: &Connection,
    session_id: &str,
    table: &str,
    id: i64,
    data: &HashMap<String, serde_json::Value>,
) -> Result<UpdateResult, Error> {
    let def = lookup_table(table)?;

    if data.is_empty() {
        return Err(Error::Db("no columns to update".into()));
    }

    if data.contains_key("session_id") {
        return Err(Error::Db("session_id cannot be updated".into()));
    }

    if data.contains_key("id") {
        return Err(Error::Db("id cannot be updated".into()));
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

    let mut set_clauses = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    for col in def.columns {
        if *col == "ip" && IP_RESOLVABLE_TABLES.contains(&table) { continue; }
        if let Some(val) = data.get(*col) {
            set_clauses.push(format!("{col} = ?{idx}"));
            values.push(json_to_sql(val));
            idx += 1;
        }
    }

    values.push(Box::new(id));
    let id_idx = idx;
    idx += 1;
    values.push(Box::new(session_id.to_string()));
    let sid_idx = idx;

    let sql = format!(
        "UPDATE {table} SET {sets} WHERE id = ?{id_idx} AND session_id = ?{sid_idx}",
        sets = set_clauses.join(", "),
    );

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let rows_changed = conn
        .execute(&sql, params_from_iter(params.iter()))
        .map_err(|e| Error::Db(e.to_string()))?;

    Ok(UpdateResult { updated: rows_changed > 0 })
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

    #[test]
    fn create_port_with_ip_auto_creates_host() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.5")), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))]);
        let r = create(&conn, "s1", "ports", &data).unwrap();
        assert!(r.created);
        let host_id: i64 = conn.query_row("SELECT id FROM hosts WHERE session_id='s1' AND ip='10.10.10.5'", [], |r| r.get(0)).unwrap();
        let port_host: i64 = conn.query_row("SELECT host_id FROM ports WHERE id=?1", [r.id], |r| r.get(0)).unwrap();
        assert_eq!(host_id, port_host);
    }

    #[test]
    fn create_port_with_ip_reuses_existing_host() {
        let conn = setup();
        conn.execute("INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')", []).unwrap();
        let existing_host_id: i64 = conn.query_row("SELECT id FROM hosts WHERE ip='10.10.10.1'", [], |r| r.get(0)).unwrap();
        let data = map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(80)), ("protocol", serde_json::json!("tcp"))]);
        let r = create(&conn, "s1", "ports", &data).unwrap();
        assert!(r.created);
        let port_host: i64 = conn.query_row("SELECT host_id FROM ports WHERE id=?1", [r.id], |r| r.get(0)).unwrap();
        assert_eq!(existing_host_id, port_host);
    }

    #[test]
    fn create_web_path_with_ip_resolves_host() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.6")), ("port", serde_json::json!(443)), ("scheme", serde_json::json!("https")), ("path", serde_json::json!("/api"))]);
        let r = create(&conn, "s1", "web_paths", &data).unwrap();
        assert!(r.created);
        let host_id: i64 = conn.query_row("SELECT id FROM hosts WHERE ip='10.10.10.6'", [], |r| r.get(0)).unwrap();
        let wp_host: i64 = conn.query_row("SELECT host_id FROM web_paths WHERE id=?1", [r.id], |r| r.get(0)).unwrap();
        assert_eq!(host_id, wp_host);
    }

    #[test]
    fn create_vuln_with_ip_resolves_host() {
        let conn = setup();
        let data = map(&[("ip", serde_json::json!("10.10.10.7")), ("port", serde_json::json!(80)), ("name", serde_json::json!("SQLi")), ("severity", serde_json::json!("high"))]);
        let r = create(&conn, "s1", "vulns", &data).unwrap();
        assert!(r.created);
        let host_id: i64 = conn.query_row("SELECT id FROM hosts WHERE ip='10.10.10.7'", [], |r| r.get(0)).unwrap();
        let vuln_host: i64 = conn.query_row("SELECT host_id FROM vulns WHERE id=?1", [r.id], |r| r.get(0)).unwrap();
        assert_eq!(host_id, vuln_host);
    }

    #[test]
    fn create_rejects_ip_and_host_id_together() {
        let conn = setup();
        conn.execute("INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')", []).unwrap();
        let data = map(&[("ip", serde_json::json!("10.10.10.1")), ("host_id", serde_json::json!(1)), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))]);
        let err = create(&conn, "s1", "ports", &data).unwrap_err();
        assert!(err.to_string().contains("either 'ip' or 'host_id'"));
    }

    #[test]
    fn create_evidence_validates_hypothesis_session() {
        let conn = setup();
        conn.execute("INSERT INTO sessions (id, name) VALUES ('s2', 'other')", []).unwrap();
        conn.execute("INSERT INTO hypotheses (session_id, statement, category) VALUES ('s2', 'test', 'auth')", []).unwrap();
        let hyp_id: i64 = conn.query_row("SELECT id FROM hypotheses WHERE session_id='s2'", [], |r| r.get(0)).unwrap();
        let data = map(&[("hypothesis_id", serde_json::json!(hyp_id)), ("finding", serde_json::json!("found")), ("severity", serde_json::json!("info"))]);
        let err = create(&conn, "s1", "evidence", &data).unwrap_err();
        assert!(err.to_string().contains("does not belong to session"));
    }

    #[test]
    fn create_evidence_with_valid_hypothesis() {
        let conn = setup();
        conn.execute("INSERT INTO hypotheses (session_id, statement, category) VALUES ('s1', 'test', 'auth')", []).unwrap();
        let hyp_id: i64 = conn.query_row("SELECT id FROM hypotheses WHERE session_id='s1'", [], |r| r.get(0)).unwrap();
        let data = map(&[("hypothesis_id", serde_json::json!(hyp_id)), ("finding", serde_json::json!("found")), ("severity", serde_json::json!("info"))]);
        let r = create(&conn, "s1", "evidence", &data).unwrap();
        assert!(r.created);
    }

    #[test]
    fn create_evidence_with_null_hypothesis_id_ok() {
        let conn = setup();
        let data = map(&[("hypothesis_id", serde_json::Value::Null), ("finding", serde_json::json!("found")), ("severity", serde_json::json!("info"))]);
        let r = create(&conn, "s1", "evidence", &data).unwrap();
        assert!(r.created);
    }

    // --- query tests ---

    #[test]
    fn query_returns_all_columns_as_json() {
        let conn = setup();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1")), ("os", serde_json::json!("Linux"))])).unwrap();
        let rows = query(&conn, "s1", "hosts", &HashMap::new()).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert!(row.get("id").is_some());
        assert_eq!(row["ip"], "10.10.10.1");
        assert_eq!(row["os"], "Linux");
        assert_eq!(row["session_id"], "s1");
    }

    #[test]
    fn query_returns_timestamps() {
        let conn = setup();
        create(&conn, "s1", "notes", &map(&[("text", serde_json::json!("hello"))])).unwrap();
        let rows = query(&conn, "s1", "notes", &HashMap::new()).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].get("created_at").is_some());
        assert!(!rows[0]["created_at"].is_null());
    }

    #[test]
    fn query_filters_by_session() {
        let conn = setup();
        conn.execute("INSERT INTO sessions (id, name) VALUES ('s2', 'other')", []).unwrap();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        create(&conn, "s2", "hosts", &map(&[("ip", serde_json::json!("10.10.10.2"))])).unwrap();
        let rows = query(&conn, "s1", "hosts", &HashMap::new()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ip"], "10.10.10.1");
    }

    #[test]
    fn query_with_key_value_filter() {
        let conn = setup();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1")), ("os", serde_json::json!("Linux"))])).unwrap();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.2")), ("os", serde_json::json!("Windows"))])).unwrap();
        let filters = map(&[("os", serde_json::json!("Linux"))]);
        let rows = query(&conn, "s1", "hosts", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ip"], "10.10.10.1");
    }

    #[test]
    fn query_with_and_semantics() {
        let conn = setup();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1")), ("os", serde_json::json!("Linux")), ("status", serde_json::json!("up"))])).unwrap();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.2")), ("os", serde_json::json!("Linux")), ("status", serde_json::json!("down"))])).unwrap();
        let filters = map(&[("os", serde_json::json!("Linux")), ("status", serde_json::json!("up"))]);
        let rows = query(&conn, "s1", "hosts", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ip"], "10.10.10.1");
    }

    #[test]
    fn query_filter_by_id() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.2"))])).unwrap();
        let filters = map(&[("id", serde_json::json!(r.id))]);
        let rows = query(&conn, "s1", "hosts", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ip"], "10.10.10.1");
    }

    #[test]
    fn query_rejects_unknown_filter_key() {
        let conn = setup();
        let filters = map(&[("bogus", serde_json::json!("x"))]);
        let err = query(&conn, "s1", "hosts", &filters).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn query_rejects_unknown_table() {
        let conn = setup();
        let err = query(&conn, "s1", "nonexistent", &HashMap::new()).unwrap_err();
        assert!(err.to_string().contains("unknown table"));
    }

    #[test]
    fn query_rejects_protected_table() {
        let conn = setup();
        let err = query(&conn, "s1", "sessions", &HashMap::new()).unwrap_err();
        assert!(err.to_string().contains("protected"));
    }

    #[test]
    fn query_ip_filter_joins_hosts_for_ports() {
        let conn = setup();
        create(&conn, "s1", "ports", &map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))])).unwrap();
        create(&conn, "s1", "ports", &map(&[("ip", serde_json::json!("10.10.10.2")), ("port", serde_json::json!(80)), ("protocol", serde_json::json!("tcp"))])).unwrap();
        let filters = map(&[("ip", serde_json::json!("10.10.10.1"))]);
        let rows = query(&conn, "s1", "ports", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["port"], 22);
    }

    #[test]
    fn query_ip_filter_joins_hosts_for_vulns() {
        let conn = setup();
        create(&conn, "s1", "vulns", &map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(80)), ("name", serde_json::json!("XSS")), ("severity", serde_json::json!("high"))])).unwrap();
        create(&conn, "s1", "vulns", &map(&[("ip", serde_json::json!("10.10.10.2")), ("port", serde_json::json!(80)), ("name", serde_json::json!("SQLi")), ("severity", serde_json::json!("critical"))])).unwrap();
        let filters = map(&[("ip", serde_json::json!("10.10.10.2"))]);
        let rows = query(&conn, "s1", "vulns", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "SQLi");
    }

    #[test]
    fn query_ip_filter_joins_hosts_for_web_paths() {
        let conn = setup();
        create(&conn, "s1", "web_paths", &map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(80)), ("scheme", serde_json::json!("http")), ("path", serde_json::json!("/admin"))])).unwrap();
        create(&conn, "s1", "web_paths", &map(&[("ip", serde_json::json!("10.10.10.2")), ("port", serde_json::json!(443)), ("scheme", serde_json::json!("https")), ("path", serde_json::json!("/api"))])).unwrap();
        let filters = map(&[("ip", serde_json::json!("10.10.10.2"))]);
        let rows = query(&conn, "s1", "web_paths", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["path"], "/api");
    }

    #[test]
    fn query_ip_filter_combined_with_other_filters() {
        let conn = setup();
        create(&conn, "s1", "ports", &map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))])).unwrap();
        create(&conn, "s1", "ports", &map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(80)), ("protocol", serde_json::json!("tcp"))])).unwrap();
        let filters = map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(22))]);
        let rows = query(&conn, "s1", "ports", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["port"], 22);
    }

    #[test]
    fn query_empty_result() {
        let conn = setup();
        let rows = query(&conn, "s1", "hosts", &HashMap::new()).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn query_all_10_tables() {
        let conn = setup();
        for (name, _) in TABLES {
            let rows = query(&conn, "s1", name, &HashMap::new()).unwrap();
            assert!(rows.is_empty(), "table {name} should return empty vec");
        }
    }

    // --- update tests ---

    #[test]
    fn update_modifies_writable_column() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1")), ("os", serde_json::json!("Linux"))])).unwrap();
        let ur = update(&conn, "s1", "hosts", r.id, &map(&[("os", serde_json::json!("Windows"))])).unwrap();
        assert!(ur.updated);
        let rows = query(&conn, "s1", "hosts", &map(&[("id", serde_json::json!(r.id))])).unwrap();
        assert_eq!(rows[0]["os"], "Windows");
    }

    #[test]
    fn update_rejects_unknown_column() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let err = update(&conn, "s1", "hosts", r.id, &map(&[("bogus", serde_json::json!("x"))])).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn update_validates_enum_constraint() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let err = update(&conn, "s1", "hosts", r.id, &map(&[("status", serde_json::json!("maybe"))])).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn update_validates_range_constraint() {
        let conn = setup();
        let r = create(&conn, "s1", "ports", &map(&[("ip", serde_json::json!("10.10.10.1")), ("port", serde_json::json!(22)), ("protocol", serde_json::json!("tcp"))])).unwrap();
        let err = update(&conn, "s1", "ports", r.id, &map(&[("port", serde_json::json!(99999))])).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn update_nonexistent_id_returns_not_updated() {
        let conn = setup();
        let ur = update(&conn, "s1", "hosts", 9999, &map(&[("os", serde_json::json!("Linux"))])).unwrap();
        assert!(!ur.updated);
    }

    #[test]
    fn update_wrong_session_returns_not_updated() {
        let conn = setup();
        conn.execute("INSERT INTO sessions (id, name) VALUES ('s2', 'other')", []).unwrap();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let ur = update(&conn, "s2", "hosts", r.id, &map(&[("os", serde_json::json!("Linux"))])).unwrap();
        assert!(!ur.updated);
    }

    #[test]
    fn update_rejects_session_id_column() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let err = update(&conn, "s1", "hosts", r.id, &map(&[("session_id", serde_json::json!("s2"))])).unwrap_err();
        assert!(err.to_string().contains("session_id cannot be updated"));
    }

    #[test]
    fn update_rejects_id_column() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let err = update(&conn, "s1", "hosts", r.id, &map(&[("id", serde_json::json!(99))])).unwrap_err();
        assert!(err.to_string().contains("id cannot be updated"));
    }

    #[test]
    fn update_rejects_empty_data() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let err = update(&conn, "s1", "hosts", r.id, &HashMap::new()).unwrap_err();
        assert!(err.to_string().contains("no columns to update"));
    }

    #[test]
    fn update_rejects_protected_table() {
        let conn = setup();
        let err = update(&conn, "s1", "sessions", 1, &map(&[("name", serde_json::json!("x"))])).unwrap_err();
        assert!(err.to_string().contains("protected"));
    }

    #[test]
    fn update_rejects_unknown_table() {
        let conn = setup();
        let err = update(&conn, "s1", "nonexistent", 1, &map(&[("x", serde_json::json!("y"))])).unwrap_err();
        assert!(err.to_string().contains("unknown table"));
    }

    #[test]
    fn update_multiple_columns() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let ur = update(&conn, "s1", "hosts", r.id, &map(&[("os", serde_json::json!("FreeBSD")), ("hostname", serde_json::json!("target")), ("status", serde_json::json!("up"))])).unwrap();
        assert!(ur.updated);
        let rows = query(&conn, "s1", "hosts", &map(&[("id", serde_json::json!(r.id))])).unwrap();
        assert_eq!(rows[0]["os"], "FreeBSD");
        assert_eq!(rows[0]["hostname"], "target");
        assert_eq!(rows[0]["status"], "up");
    }

    #[test]
    fn update_null_value_clears_field() {
        let conn = setup();
        let r = create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1")), ("os", serde_json::json!("Linux"))])).unwrap();
        let ur = update(&conn, "s1", "hosts", r.id, &map(&[("os", serde_json::Value::Null)])).unwrap();
        assert!(ur.updated);
        let rows = query(&conn, "s1", "hosts", &map(&[("id", serde_json::json!(r.id))])).unwrap();
        assert!(rows[0]["os"].is_null());
    }

    #[test]
    fn update_hypothesis_status() {
        let conn = setup();
        let r = create(&conn, "s1", "hypotheses", &map(&[("statement", serde_json::json!("SSH weak")), ("category", serde_json::json!("auth")), ("status", serde_json::json!("pending"))])).unwrap();
        let ur = update(&conn, "s1", "hypotheses", r.id, &map(&[("status", serde_json::json!("confirmed")), ("confidence", serde_json::json!(0.95))])).unwrap();
        assert!(ur.updated);
        let rows = query(&conn, "s1", "hypotheses", &map(&[("id", serde_json::json!(r.id))])).unwrap();
        assert_eq!(rows[0]["status"], "confirmed");
    }

    #[test]
    fn query_ip_not_virtual_on_hosts() {
        let conn = setup();
        create(&conn, "s1", "hosts", &map(&[("ip", serde_json::json!("10.10.10.1"))])).unwrap();
        let filters = map(&[("ip", serde_json::json!("10.10.10.1"))]);
        let rows = query(&conn, "s1", "hosts", &filters).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ip"], "10.10.10.1");
    }
}
