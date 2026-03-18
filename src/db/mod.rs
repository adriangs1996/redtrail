pub mod commands;
pub mod hypothesis;
pub mod kb;
pub mod session;

use crate::error::Error;
use rusqlite::Connection;

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

    pub(crate) fn open_in_memory() -> Result<Self, Error> {
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

    pub(crate) fn conn(&self) -> &Connection { &self.conn }

    pub fn add_host(&self, session_id: &str, ip: &str, os: Option<&str>, hostname: Option<&str>) -> Result<i64, Error> {
        kb::add_host(&self.conn, session_id, ip, os, hostname)
    }
    pub fn add_port(&self, session_id: &str, host_ip: &str, port: i64, protocol: Option<&str>, service: Option<&str>, version: Option<&str>) -> Result<i64, Error> {
        kb::add_port(&self.conn, session_id, host_ip, port, protocol, service, version)
    }
    pub fn add_credential(&self, session_id: &str, username: &str, password: Option<&str>, hash: Option<&str>, service: Option<&str>, host: Option<&str>, source: Option<&str>) -> Result<i64, Error> {
        kb::add_credential(&self.conn, session_id, username, password, hash, service, host, source)
    }
    pub fn add_flag(&self, session_id: &str, value: &str, source: Option<&str>) -> Result<i64, Error> {
        kb::add_flag(&self.conn, session_id, value, source)
    }
    pub fn add_access(&self, session_id: &str, host: &str, user: &str, level: &str, method: Option<&str>) -> Result<i64, Error> {
        kb::add_access(&self.conn, session_id, host, user, level, method)
    }
    pub fn add_note(&self, session_id: &str, text: &str) -> Result<i64, Error> {
        kb::add_note(&self.conn, session_id, text)
    }
    pub fn list_hosts(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_hosts(&self.conn, session_id)
    }
    pub fn list_ports(&self, session_id: &str, host_filter: Option<&str>) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_ports(&self.conn, session_id, host_filter)
    }
    pub fn list_credentials(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_credentials(&self.conn, session_id)
    }
    pub fn list_flags(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_flags(&self.conn, session_id)
    }
    pub fn list_access(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_access(&self.conn, session_id)
    }
    pub fn list_notes(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_notes(&self.conn, session_id)
    }
    pub fn list_history(&self, session_id: &str, limit: usize) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_history(&self.conn, session_id, limit)
    }
    pub fn search(&self, session_id: &str, query: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::search(&self.conn, session_id, query)
    }

    pub fn create_hypothesis(&self, session_id: &str, statement: &str, category: &str, priority: &str, confidence: f64, target_component: Option<&str>) -> Result<i64, Error> {
        hypothesis::create(&self.conn, session_id, statement, category, priority, confidence, target_component)
    }
    pub fn list_hypotheses(&self, session_id: &str, status_filter: Option<&str>) -> Result<Vec<serde_json::Value>, Error> {
        hypothesis::list(&self.conn, session_id, status_filter)
    }
    pub fn update_hypothesis(&self, id: i64, status: &str) -> Result<(), Error> {
        hypothesis::update_status(&self.conn, id, status)
    }
    pub fn get_hypothesis(&self, id: i64) -> Result<serde_json::Value, Error> {
        hypothesis::get(&self.conn, id)
    }
    pub fn create_evidence(&self, session_id: &str, hypothesis_id: Option<i64>, finding: &str, severity: &str, poc: Option<&str>) -> Result<i64, Error> {
        hypothesis::create_evidence(&self.conn, session_id, hypothesis_id, finding, severity, poc)
    }
    pub fn list_evidence(&self, session_id: &str, hypothesis_id: Option<i64>) -> Result<Vec<serde_json::Value>, Error> {
        hypothesis::list_evidence(&self.conn, session_id, hypothesis_id)
    }
    pub fn export_evidence(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        hypothesis::export_evidence(&self.conn, session_id)
    }

    pub fn insert_command(&self, session_id: &str, command: &str, tool: Option<&str>) -> Result<i64, Error> {
        commands::insert(&self.conn, session_id, command, tool)
    }
    pub fn finish_command(&self, id: i64, exit_code: i32, duration_ms: i64, output: &str) -> Result<(), Error> {
        commands::finish(&self.conn, id, exit_code, duration_ms, output)
    }
    pub fn get_command_for_extraction(&self, id: i64) -> Result<(String, String, Option<String>, Option<String>), Error> {
        commands::get_for_extraction(&self.conn, id)
    }
    pub fn update_extraction_status(&self, id: i64, status: &str) -> Result<(), Error> {
        commands::update_extraction_status(&self.conn, id, status)
    }

    pub fn active_session_id(&self) -> Result<String, Error> {
        session::active_session_id(&self.conn)
    }
    pub fn load_flag_patterns(&self, session_id: &str) -> Result<Vec<String>, Error> {
        session::load_flag_patterns(&self.conn, session_id)
    }
    pub fn load_scope(&self, session_id: &str) -> Result<Option<String>, Error> {
        session::load_scope(&self.conn, session_id)
    }
    pub fn decrement_noise_budget(&self, session_id: &str, cost: f64) -> Result<(), Error> {
        session::decrement_noise_budget(&self.conn, session_id, cost)
    }
    pub fn status_summary(&self, session_id: &str) -> Result<serde_json::Value, Error> {
        session::status_summary(&self.conn, session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

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
