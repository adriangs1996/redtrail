use crate::error::Error;
use crate::resolve;
use rusqlite::Connection;

pub fn run(sql: &str, json: bool) -> Result<(), Error> {
    let conn = open_db()?;
    let output = execute_to_string(&conn, sql)?;
    if json {
        let result = execute_query(&conn, sql)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&result.to_json()).unwrap()
        );
    } else {
        print!("{output}");
    }
    Ok(())
}

pub fn run_file(path: &str, json: bool) -> Result<(), Error> {
    let sql = std::fs::read_to_string(path)?;
    run(sql.trim(), json)
}

fn open_db() -> Result<Connection, Error> {
    let db_path = resolve::global_db_path()?;
    crate::db::open_connection(db_path.to_str().unwrap())
}

struct QueryResult {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    affected: usize,
    is_query: bool,
}

impl QueryResult {
    fn to_json(&self) -> serde_json::Value {
        if !self.is_query {
            return serde_json::json!({"affected_rows": self.affected});
        }
        let json_rows: Vec<serde_json::Value> = self
            .rows
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for (i, val) in row.iter().enumerate() {
                    map.insert(
                        self.columns[i].clone(),
                        serde_json::Value::String(val.clone()),
                    );
                }
                serde_json::Value::Object(map)
            })
            .collect();
        serde_json::json!(json_rows)
    }
}

fn is_read_query(sql: &str) -> bool {
    let upper = sql.trim_start().to_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("PRAGMA")
        || upper.starts_with("EXPLAIN")
        || upper.starts_with("WITH")
}

fn execute_query(conn: &Connection, sql: &str) -> Result<QueryResult, Error> {
    if is_read_query(sql) {
        let mut stmt = conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
        let col_count = stmt.column_count();
        let columns: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let rows: Vec<Vec<String>> = stmt
            .query_map([], |row| {
                let mut vals = Vec::new();
                for i in 0..col_count {
                    let val: String = row
                        .get::<_, rusqlite::types::Value>(i)
                        .map(|v| match v {
                            rusqlite::types::Value::Null => "NULL".to_string(),
                            rusqlite::types::Value::Integer(n) => n.to_string(),
                            rusqlite::types::Value::Real(f) => f.to_string(),
                            rusqlite::types::Value::Text(s) => s,
                            rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
                        })
                        .unwrap_or_else(|_| "?".to_string());
                    vals.push(val);
                }
                Ok(vals)
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(QueryResult {
            columns,
            rows,
            affected: 0,
            is_query: true,
        })
    } else {
        let affected = conn
            .execute(sql, [])
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            affected,
            is_query: false,
        })
    }
}

pub fn execute_to_string(conn: &Connection, sql: &str) -> Result<String, Error> {
    let result = execute_query(conn, sql)?;

    if !result.is_query {
        return Ok(format!("{} rows affected\n", result.affected));
    }

    if result.rows.is_empty() {
        return Ok("(0 rows)\n".to_string());
    }

    let mut widths: Vec<usize> = result.columns.iter().map(|n| n.len()).collect();
    for row in &result.rows {
        for (i, val) in row.iter().enumerate() {
            widths[i] = widths[i].max(val.len());
        }
    }

    let mut out = String::new();
    let header: Vec<String> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, n)| format!("{:<width$}", n, width = widths[i]))
        .collect();
    out.push_str(&header.join(" | "));
    out.push('\n');
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&sep.join("-+-"));
    out.push('\n');

    for row in &result.rows {
        let formatted: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, v)| format!("{:<width$}", v, width = widths[i]))
            .collect();
        out.push_str(&formatted.join(" | "));
        out.push('\n');
    }
    out.push_str(&format!("({} rows)\n", result.rows.len()));

    Ok(out)
}
