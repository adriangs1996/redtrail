use crate::db_v2::DbV2;
use crate::error::Error;

pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn run(db: &DbV2, sql: &str) -> Result<QueryResult, Error> {
    let conn = db.conn();
    let mut stmt = conn.prepare(sql)
        .map_err(|e| Error::Db(e.to_string()))?;

    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let col_count = columns.len();

    let rows: Vec<Vec<String>> = stmt.query_map([], |row| {
        let mut vals = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let val: String = row.get::<_, rusqlite::types::Value>(i)
                .map(|v| match v {
                    rusqlite::types::Value::Null => "NULL".to_string(),
                    rusqlite::types::Value::Integer(i) => i.to_string(),
                    rusqlite::types::Value::Real(f) => f.to_string(),
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
                })
                .unwrap_or_else(|_| "?".to_string());
            vals.push(val);
        }
        Ok(vals)
    }).map_err(|e| Error::Db(e.to_string()))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(QueryResult { columns, rows })
}
