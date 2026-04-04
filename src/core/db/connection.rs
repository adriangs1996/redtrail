use crate::error::Error;
use rusqlite::Connection;

use super::schema::{init, set_file_permissions};

pub fn open(path: &str) -> Result<Connection, Error> {
    let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
    init(&conn)?;
    set_file_permissions(path);
    Ok(conn)
}

/// Lightweight open for processes that only need to read/write existing data
/// (e.g. tee). Skips schema creation, migrations, and PRAGMA optimize to avoid
/// contention with the main CLI process that already initialised the DB.
pub fn open_existing(path: &str) -> Result<Connection, Error> {
    let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA busy_timeout=3000;")
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection, Error> {
    let conn = Connection::open_in_memory().map_err(|e| Error::Db(e.to_string()))?;
    init(&conn)?;
    Ok(conn)
}

pub fn global_db_path() -> Result<std::path::PathBuf, Error> {
    let home = std::env::var("HOME").map_err(|_| Error::Config("HOME not set".into()))?;
    let dir = std::path::PathBuf::from(home).join(".local/share/redtrail");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("redtrail.db"))
}
