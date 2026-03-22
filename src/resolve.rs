use crate::config::Config;
use crate::error::Error;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub struct SessionContext {
    pub conn: Connection,
    pub session_id: String,
    pub workspace_path: PathBuf,
    pub config: Config,
}

pub struct GlobalContext {
    pub conn: Connection,
}

impl GlobalContext {
    pub fn find_session(&self, cwd: &Path) -> Result<Option<(String, PathBuf)>, Error> {
        let path_str = cwd.to_string_lossy();
        let result = self.conn.query_row(
            "SELECT id, workspace_path FROM sessions WHERE workspace_path = ?1 AND active = 1",
            rusqlite::params![path_str.as_ref()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        match result {
            Ok((id, wp)) => Ok(Some((id, PathBuf::from(wp)))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Db(e.to_string())),
        }
    }
}

pub fn global_db_path() -> Result<PathBuf, Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| Error::Config("cannot determine home directory".into()))?;
    Ok(home.join(".redtrail/redtrail.db"))
}

pub fn resolve_global() -> Result<GlobalContext, Error> {
    let db_path = global_db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&db_path).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(crate::db::SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(GlobalContext { conn })
}

pub fn resolve(cwd: &Path) -> Result<SessionContext, Error> {
    let ctx = resolve_global()?;
    let cwd = cwd.canonicalize().map_err(|_| Error::NoWorkspace)?;

    let mut dir = cwd.clone();
    loop {
        if let Some((session_id, workspace_path)) = ctx.find_session(&dir)? {
            let GlobalContext { conn } = ctx;
            let config = Config::resolved(&conn, &session_id)?;
            return Ok(SessionContext {
                conn,
                session_id,
                workspace_path,
                config,
            });
        }
        if !dir.pop() {
            break;
        }
    }
    Err(Error::NoActiveSession)
}

#[cfg(test)]
pub fn resolve_global_in_memory() -> Result<GlobalContext, Error> {
    let conn = Connection::open_in_memory().map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(crate::db::SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(GlobalContext { conn })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_global_in_memory() {
        let ctx = resolve_global_in_memory().unwrap();
        let tables: i64 = ctx.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(tables > 0);
    }

    #[test]
    fn test_find_session_empty() {
        let ctx = resolve_global_in_memory().unwrap();
        let result = ctx.find_session(Path::new("/tmp/nonexistent")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_found() {
        let ctx = resolve_global_in_memory().unwrap();
        ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["s1", "test", "/tmp/test", "10.10.10.1", "", "general"],
        ).unwrap();
        let result = ctx.find_session(Path::new("/tmp/test")).unwrap();
        assert!(result.is_some());
        let (id, wp) = result.unwrap();
        assert_eq!(id, "s1");
        assert_eq!(wp, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_find_session_inactive_not_returned() {
        let ctx = resolve_global_in_memory().unwrap();
        ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, active, target, scope, goal) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            rusqlite::params!["s1", "test", "/tmp/test", "10.10.10.1", "", "general"],
        ).unwrap();
        let result = ctx.find_session(Path::new("/tmp/test")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_partial_unique_index_prevents_two_active() {
        let ctx = resolve_global_in_memory().unwrap();
        ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["s1", "first", "/tmp/test", "", "", "general"],
        ).unwrap();
        let result = ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["s2", "second", "/tmp/test", "", "", "general"],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_two_inactive_same_path_allowed() {
        let ctx = resolve_global_in_memory().unwrap();
        ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, active, target, scope, goal) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            rusqlite::params!["s1", "first", "/tmp/test", "", "", "general"],
        ).unwrap();
        ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, active, target, scope, goal) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            rusqlite::params!["s2", "second", "/tmp/test", "", "", "general"],
        ).unwrap();
    }

    #[test]
    fn test_global_config_table_exists() {
        let ctx = resolve_global_in_memory().unwrap();
        ctx.conn.execute(
            "INSERT INTO global_config (key, value) VALUES ('test.key', 'test_value')",
            [],
        ).unwrap();
        let val: String = ctx.conn.query_row(
            "SELECT value FROM global_config WHERE key = 'test.key'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "test_value");
    }

    #[test]
    fn test_session_config_table_exists() {
        let ctx = resolve_global_in_memory().unwrap();
        ctx.conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES ('s1', 'test', '/tmp', '', '', 'general')",
            [],
        ).unwrap();
        ctx.conn.execute(
            "INSERT INTO session_config (session_id, key, value) VALUES ('s1', 'general.autonomy', 'cautious')",
            [],
        ).unwrap();
        let val: String = ctx.conn.query_row(
            "SELECT value FROM session_config WHERE session_id = 's1' AND key = 'general.autonomy'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "cautious");
    }
}
