use redtrail::context::AppContext;
use redtrail::core::fmt;
use redtrail::error::Error;

pub struct SqlArgs {
    pub query: String,
    pub json: bool,
}

pub fn run(ctx: &AppContext, args: &SqlArgs) -> Result<(), Error> {
    let sql = args.query.trim();

    if is_read_query(sql) {
        let mut stmt = ctx.conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
        let col_count = stmt.column_count();
        let columns: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let rows: Vec<Vec<serde_json::Value>> = stmt
            .query_map([], |row| {
                let mut vals = Vec::new();
                for i in 0..col_count {
                    let val = row
                        .get::<_, rusqlite::types::Value>(i)
                        .map(|v| match v {
                            rusqlite::types::Value::Null => serde_json::Value::Null,
                            rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                            rusqlite::types::Value::Real(f) => serde_json::json!(f),
                            rusqlite::types::Value::Text(s) => serde_json::json!(s),
                            rusqlite::types::Value::Blob(b) => serde_json::json!(format!("<blob {} bytes>", b.len())),
                        })
                        .unwrap_or(serde_json::Value::Null);
                    vals.push(val);
                }
                Ok(vals)
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if args.json {
            let json_rows: Vec<serde_json::Value> = rows.iter().map(|row| {
                let mut map = serde_json::Map::new();
                for (i, val) in row.iter().enumerate() {
                    if i < columns.len() {
                        map.insert(columns[i].clone(), val.clone());
                    }
                }
                serde_json::Value::Object(map)
            }).collect();
            println!("{}", serde_json::to_string_pretty(&json_rows).unwrap());
        } else {
            print!("{}", fmt::format("table", &columns, &rows));
        }
    } else {
        let affected = ctx.conn.execute(sql, []).map_err(|e| Error::Db(e.to_string()))?;
        if args.json {
            println!("{}", serde_json::json!({"affected_rows": affected}));
        } else {
            println!("{affected} rows affected");
        }
    }

    Ok(())
}

fn is_read_query(sql: &str) -> bool {
    let upper = sql.trim_start().to_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("PRAGMA")
        || upper.starts_with("EXPLAIN")
        || upper.starts_with("WITH")
}
