use crate::error::Error;
use rusqlite::Connection;

pub fn run(conn: &Connection, sql: &str, json: bool) -> Result<(), Error> {
    let trimmed = sql.trim();
    let upper = trimmed.to_uppercase();
    if !upper.starts_with("SELECT") && !upper.starts_with("WITH") && !upper.starts_with("EXPLAIN") {
        eprintln!("only SELECT queries are allowed. Got: {}", trimmed.split_whitespace().next().unwrap_or(""));
        std::process::exit(1);
    }

    let mut stmt = conn.prepare(trimmed).map_err(|e| Error::Db(e.to_string()))?;
    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let rows: Vec<Vec<serde_json::Value>> = stmt
        .query_map([], |row| {
            let mut vals = Vec::new();
            for i in 0..col_count {
                let val: rusqlite::types::Value = row.get_unwrap(i);
                vals.push(match val {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                    rusqlite::types::Value::Real(f) => serde_json::json!(f),
                    rusqlite::types::Value::Text(s) => serde_json::json!(s),
                    rusqlite::types::Value::Blob(b) => serde_json::json!(format!("<blob:{} bytes>", b.len())),
                });
            }
            Ok(vals)
        })
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

    if json {
        let json_rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let mut map = serde_json::Map::new();
                for (i, val) in r.iter().enumerate() {
                    map.insert(col_names[i].clone(), val.clone());
                }
                serde_json::Value::Object(map)
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_rows).unwrap());
    } else {
        // Simple table output
        // Header
        println!("{}", col_names.join("\t"));
        for row in &rows {
            let cells: Vec<String> = row.iter().map(|v| match v {
                serde_json::Value::Null => "NULL".to_string(),
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }).collect();
            println!("{}", cells.join("\t"));
        }
    }

    Ok(())
}
