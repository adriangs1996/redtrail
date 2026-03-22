pub mod chat;
pub mod commands;
pub mod config;
pub mod dispatcher;
pub mod hypothesis;
pub mod kb;
pub mod briefing;
pub mod schema;
pub(crate) mod session;

use crate::error::Error;
use rusqlite::Connection;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    workspace_path TEXT NOT NULL,
    active INTEGER DEFAULT 1,
    target TEXT,
    scope TEXT,
    goal TEXT DEFAULT 'general',
    goal_meta TEXT DEFAULT '{}',
    phase TEXT DEFAULT 'L0',
    noise_budget REAL DEFAULT 1.0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(name, workspace_path)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_one_active_per_workspace
    ON sessions(workspace_path) WHERE active = 1;

CREATE TABLE IF NOT EXISTS global_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_config (
    session_id TEXT NOT NULL REFERENCES sessions(id),
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (session_id, key)
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

CREATE TABLE IF NOT EXISTS web_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    host_id INTEGER NOT NULL REFERENCES hosts(id),
    port INTEGER NOT NULL DEFAULT 80,
    scheme TEXT NOT NULL DEFAULT 'http',
    path TEXT NOT NULL,
    status_code INTEGER,
    content_length INTEGER,
    content_type TEXT,
    redirect_to TEXT,
    source TEXT,
    found_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, host_id, port, path)
);

CREATE TABLE IF NOT EXISTS vulns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    host_id INTEGER NOT NULL REFERENCES hosts(id),
    port INTEGER NOT NULL DEFAULT 0,
    name TEXT NOT NULL,
    severity TEXT,
    cve TEXT,
    url TEXT,
    detail TEXT,
    source TEXT,
    found_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, host_id, port, name)
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now'))
);
";

pub trait KnowledgeBase {
    fn list_hosts(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error>;
    fn list_ports(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error>;
    fn list_credentials(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error>;
    fn list_flags(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error>;
    fn list_access(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error>;
    fn list_notes(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error>;
    fn list_web_paths(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error>;
    fn list_vulns(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
        severity_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error>;
    fn list_history(&self, session_id: &str, limit: usize)
    -> Result<Vec<serde_json::Value>, Error>;
    fn search(&self, session_id: &str, query: &str) -> Result<Vec<serde_json::Value>, Error>;
}

pub trait Hypotheses {
    fn list_hypotheses(
        &self,
        session_id: &str,
        status_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error>;
    fn get_hypothesis(&self, id: i64) -> Result<serde_json::Value, Error>;
    fn list_evidence(
        &self,
        session_id: &str,
        hypothesis_id: Option<i64>,
    ) -> Result<Vec<serde_json::Value>, Error>;
    fn export_evidence(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error>;
}

pub trait CommandLog {
    fn insert_command(
        &self,
        session_id: &str,
        command: &str,
        tool: Option<&str>,
    ) -> Result<i64, Error>;
    fn finish_command(
        &self,
        id: i64,
        exit_code: i32,
        duration_ms: i64,
        output: &str,
    ) -> Result<(), Error>;
}

#[allow(dead_code)]
pub trait SessionOps {
    fn active_session_id(&self, workspace_path: &str) -> Result<String, Error>;
    fn create_session(
        &self,
        id: &str,
        name: &str,
        workspace_path: &str,
        target: Option<&str>,
        scope: Option<&str>,
        goal: &str,
    ) -> Result<(), Error>;
    fn deactivate_session(&self, workspace_path: &str) -> Result<(), Error>;
    fn activate_session(&self, session_id: &str) -> Result<(), Error>;
    fn get_session(&self, session_id: &str) -> Result<serde_json::Value, Error>;
    fn load_scope(&self, session_id: &str) -> Result<Option<String>, Error>;
    fn status_summary(&self, session_id: &str) -> Result<serde_json::Value, Error>;
}

struct SqliteDb {
    conn: Connection,
}

impl SqliteDb {
    fn init(&self) -> Result<(), Error> {
        self.conn
            .execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| Error::Db(e.to_string()))?;
        self.conn
            .execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| Error::Db(e.to_string()))?;
        self.conn
            .execute_batch(SCHEMA)
            .map_err(|e| Error::Db(e.to_string()))
    }
}

pub fn open(
    path: &str,
) -> Result<impl KnowledgeBase + Hypotheses + CommandLog + SessionOps + use<>, Error> {
    let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
    let db = SqliteDb { conn };
    db.init()?;
    Ok(db)
}

pub fn open_connection(path: &str) -> Result<Connection, Error> {
    let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn)
}

#[cfg(test)]
pub(crate) fn open_in_memory()
-> Result<impl KnowledgeBase + Hypotheses + CommandLog + SessionOps, Error> {
    let conn = Connection::open_in_memory().map_err(|e| Error::Db(e.to_string()))?;
    let db = SqliteDb { conn };
    db.init()?;
    Ok(db)
}

impl KnowledgeBase for SqliteDb {
    fn list_hosts(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_hosts(&self.conn, session_id)
    }
    fn list_ports(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_ports(&self.conn, session_id, host_filter)
    }
    fn list_credentials(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_credentials(&self.conn, session_id)
    }
    fn list_flags(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_flags(&self.conn, session_id)
    }
    fn list_access(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_access(&self.conn, session_id)
    }
    fn list_notes(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_notes(&self.conn, session_id)
    }
    fn list_web_paths(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_web_paths(&self.conn, session_id, host_filter)
    }
    fn list_vulns(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
        severity_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_vulns(&self.conn, session_id, host_filter, severity_filter)
    }
    fn list_history(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_history(&self.conn, session_id, limit)
    }
    fn search(&self, session_id: &str, query: &str) -> Result<Vec<serde_json::Value>, Error> {
        kb::search(&self.conn, session_id, query)
    }
}

impl Hypotheses for SqliteDb {
    fn list_hypotheses(
        &self,
        session_id: &str,
        status_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        hypothesis::list(&self.conn, session_id, status_filter)
    }
    fn get_hypothesis(&self, id: i64) -> Result<serde_json::Value, Error> {
        hypothesis::get(&self.conn, id)
    }
    fn list_evidence(
        &self,
        session_id: &str,
        hypothesis_id: Option<i64>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        hypothesis::list_evidence(&self.conn, session_id, hypothesis_id)
    }
    fn export_evidence(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        hypothesis::export_evidence(&self.conn, session_id)
    }
}

impl CommandLog for SqliteDb {
    fn insert_command(
        &self,
        session_id: &str,
        command: &str,
        tool: Option<&str>,
    ) -> Result<i64, Error> {
        commands::insert(&self.conn, session_id, command, tool)
    }
    fn finish_command(
        &self,
        id: i64,
        exit_code: i32,
        duration_ms: i64,
        output: &str,
    ) -> Result<(), Error> {
        commands::finish(&self.conn, id, exit_code, duration_ms, output)
    }
}

impl SessionOps for SqliteDb {
    fn active_session_id(&self, workspace_path: &str) -> Result<String, Error> {
        session::active_session_id(&self.conn, workspace_path)
    }
    fn create_session(
        &self,
        id: &str,
        name: &str,
        workspace_path: &str,
        target: Option<&str>,
        scope: Option<&str>,
        goal: &str,
    ) -> Result<(), Error> {
        session::create_session(&self.conn, id, name, workspace_path, target, scope, goal)
    }
    fn deactivate_session(&self, workspace_path: &str) -> Result<(), Error> {
        session::deactivate_session(&self.conn, workspace_path)
    }
    fn activate_session(&self, session_id: &str) -> Result<(), Error> {
        session::activate_session(&self.conn, session_id)
    }
    fn get_session(&self, session_id: &str) -> Result<serde_json::Value, Error> {
        session::get_session(&self.conn, session_id)
    }
    fn load_scope(&self, session_id: &str) -> Result<Option<String>, Error> {
        session::load_scope(&self.conn, session_id)
    }
    fn status_summary(&self, session_id: &str) -> Result<serde_json::Value, Error> {
        session::status_summary(&self.conn, session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_creates_schema() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general")
            .unwrap();
        let id = db.active_session_id("/tmp/test").unwrap();
        assert_eq!(id, "s1");
    }

    #[test]
    fn test_knowledge_base_via_factory() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", Some("10.10.10.1"), None, "general")
            .unwrap();
        let hosts = db.list_hosts("s1").unwrap();
        assert!(hosts.is_empty());
    }

    #[test]
    fn test_command_log_via_factory() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general")
            .unwrap();
        let id = db
            .insert_command("s1", "nmap 10.10.10.1", Some("nmap"))
            .unwrap();
        db.finish_command(id, 0, 500, "22/tcp open ssh").unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_hypotheses_list_via_factory() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general")
            .unwrap();
        let rows = db.list_hypotheses("s1", None).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_session_ops_via_factory() {
        let db = open_in_memory().unwrap();
        db.create_session(
            "s1",
            "test",
            "/tmp/test",
            Some("10.10.10.1"),
            Some("10.10.10.0/24"),
            "general",
        )
        .unwrap();
        let id = db.active_session_id("/tmp/test").unwrap();
        assert_eq!(id, "s1");
        let scope = db.load_scope("s1").unwrap();
        assert_eq!(scope.as_deref(), Some("10.10.10.0/24"));
    }

    #[test]
    fn test_deactivate_session() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general")
            .unwrap();
        db.deactivate_session("/tmp/test").unwrap();
        assert!(db.active_session_id("/tmp/test").is_err());
    }

    #[test]
    fn test_activate_session() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general")
            .unwrap();
        db.deactivate_session("/tmp/test").unwrap();
        db.activate_session("s1").unwrap();
        let id = db.active_session_id("/tmp/test").unwrap();
        assert_eq!(id, "s1");
    }
}
