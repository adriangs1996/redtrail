use crate::error::Error;
use rusqlite::Connection;

pub fn open(_path: &str) -> Result<Connection, Error> {
    todo!()
}

pub fn global_db_path() -> Result<std::path::PathBuf, Error> {
    let home = std::env::var("HOME").map_err(|_| Error::Config("HOME not set".into()))?;
    let dir = std::path::PathBuf::from(home).join(".redtrail");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("redtrail.db"))
}

pub fn ensure_session(_conn: &Connection, _workspace_path: &str) -> Result<String, Error> {
    todo!()
}
