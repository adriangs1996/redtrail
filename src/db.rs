use rusqlite::{Connection, params};
use crate::error::Error;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    target TEXT,
    scope TEXT,
    goal TEXT DEFAULT 'general',
    goal_meta TEXT DEFAULT '{}',
    phase TEXT DEFAULT 'L0',
    noise_budget REAL DEFAULT 1.0,
    autonomy TEXT DEFAULT 'balanced',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS hosts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    ip TEXT NOT NULL,
    hostname TEXT,
    os TEXT,
    status TEXT DEFAULT 'up',
    UNIQUE(session_id, ip)
);

CREATE TABLE IF NOT EXISTS ports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    host_id INTEGER NOT NULL REFERENCES hosts(id),
    port INTEGER NOT NULL,
    protocol TEXT DEFAULT 'tcp',
    service TEXT,
    version TEXT,
    UNIQUE(host_id, port, protocol)
);

CREATE TABLE IF NOT EXISTS credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    username TEXT NOT NULL,
    password TEXT,
    hash TEXT,
    service TEXT,
    host TEXT,
    source TEXT,
    found_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS access_levels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    host TEXT NOT NULL,
    user TEXT NOT NULL,
    level TEXT NOT NULL,
    method TEXT,
    obtained_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS flags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    value TEXT NOT NULL,
    source TEXT,
    captured_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS hypotheses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    statement TEXT NOT NULL,
    category TEXT NOT NULL,
    status TEXT DEFAULT 'pending',
    priority TEXT DEFAULT 'medium',
    confidence REAL DEFAULT 0.5,
    target_component TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE TABLE IF NOT EXISTS evidence (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    hypothesis_id INTEGER REFERENCES hypotheses(id),
    finding TEXT NOT NULL,
    severity TEXT DEFAULT 'info',
    poc TEXT,
    raw_output TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS command_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    command TEXT NOT NULL,
    exit_code INTEGER,
    duration_ms INTEGER,
    output TEXT,
    output_preview TEXT,
    tool TEXT,
    extraction_status TEXT DEFAULT 'pending',
    started_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    text TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now'))
);
";

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory().map_err(|e| Error::Db(e.to_string()))?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<(), Error> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| Error::Db(e.to_string()))?;
        self.conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| Error::Db(e.to_string()))?;
        self.conn.execute_batch(SCHEMA)
            .map_err(|e| Error::Db(e.to_string()))
    }

    pub fn conn(&self) -> &Connection { &self.conn }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_creates_schema() {
        let db = Db::open_in_memory().unwrap();
        let count: i32 = db.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [], |r| r.get(0),
        ).unwrap();
        assert!(count >= 10, "expected at least 10 tables, got {count}");
    }

    #[test]
    fn test_wal_mode() {
        let db = Db::open_in_memory().unwrap();
        let mode: String = db.conn.query_row(
            "PRAGMA journal_mode", [], |r| r.get(0),
        ).unwrap();
        assert!(!mode.is_empty());
    }

    #[test]
    fn test_insert_and_query_session() {
        let db = Db::open_in_memory().unwrap();
        db.conn.execute(
            "INSERT INTO sessions (id, name, target) VALUES (?1, ?2, ?3)",
            params!["s1", "test", "10.10.10.1"],
        ).unwrap();
        let name: String = db.conn.query_row(
            "SELECT name FROM sessions WHERE id = ?1",
            params!["s1"], |r| r.get(0),
        ).unwrap();
        assert_eq!(name, "test");
    }
}
