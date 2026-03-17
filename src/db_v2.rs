use rusqlite::Connection;
use crate::error::Error;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL DEFAULT 'running',
    env_json TEXT NOT NULL DEFAULT '{}',
    tool_config_json TEXT NOT NULL DEFAULT '{}',
    llm_provider TEXT NOT NULL DEFAULT 'anthropic-api',
    llm_model TEXT NOT NULL DEFAULT 'claude-opus-4-6-20250612',
    working_dir TEXT NOT NULL DEFAULT '.',
    prompt_template TEXT NOT NULL DEFAULT 'redtrail:{session} {status}$ '
);

CREATE TABLE IF NOT EXISTS hosts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    ip TEXT NOT NULL,
    hostname TEXT,
    os TEXT,
    status TEXT NOT NULL DEFAULT 'up'
);

CREATE TABLE IF NOT EXISTS ports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    host_id INTEGER NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    port INTEGER NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'tcp',
    service TEXT,
    version TEXT
);

CREATE TABLE IF NOT EXISTS credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    username TEXT NOT NULL,
    password TEXT,
    hash TEXT,
    source TEXT,
    host_id INTEGER REFERENCES hosts(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS access_levels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    host_id INTEGER NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    user TEXT NOT NULL,
    level TEXT NOT NULL,
    method TEXT
);

CREATE TABLE IF NOT EXISTS flags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    value TEXT NOT NULL,
    source TEXT,
    captured_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS attack_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    from_host TEXT,
    to_host TEXT,
    technique TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'planned'
);

CREATE TABLE IF NOT EXISTS findings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    type TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    title TEXT NOT NULL,
    description TEXT,
    evidence TEXT
);

CREATE TABLE IF NOT EXISTS command_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    command TEXT NOT NULL,
    exit_code INTEGER,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    duration_ms INTEGER,
    output_preview TEXT,
    block_id INTEGER
);

CREATE TABLE IF NOT EXISTS failed_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    technique TEXT NOT NULL,
    target TEXT,
    reason TEXT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS attack_patterns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    technique TEXT NOT NULL,
    vulnerability_class TEXT NOT NULL,
    service_type TEXT NOT NULL,
    technology_stack TEXT DEFAULT '',
    total_attempts INTEGER DEFAULT 0,
    successes INTEGER DEFAULT 0,
    avg_tool_calls REAL DEFAULT 0.0,
    avg_duration_secs REAL DEFAULT 0.0,
    brute_force_needed INTEGER DEFAULT 0,
    attack_chain TEXT DEFAULT '',
    first_seen_at TEXT DEFAULT (datetime('now')),
    last_seen_at TEXT DEFAULT (datetime('now')),
    last_session_id TEXT
);

CREATE TABLE IF NOT EXISTS technique_executions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    task_type TEXT NOT NULL,
    target_host TEXT NOT NULL,
    target_service TEXT DEFAULT '',
    tool_calls INTEGER DEFAULT 0,
    wall_clock_secs REAL DEFAULT 0.0,
    succeeded INTEGER DEFAULT 0,
    brute_force_used INTEGER DEFAULT 0,
    technology_stack TEXT DEFAULT '',
    executed_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS input_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    command TEXT NOT NULL,
    command_history_id INTEGER REFERENCES command_history(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS chat_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";

pub struct CommandResult {
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub output_preview: Option<String>,
    pub started_at: String,
}

pub struct DbV2 {
    conn: Connection,
}

impl DbV2 {
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path)
            .map_err(|e| Error::Db(e.to_string()))?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Db(e.to_string()))?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), Error> {
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| Error::Db(e.to_string()))?;
        self.conn.execute_batch(SCHEMA)
            .map_err(|e| Error::Db(e.to_string()))
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn save_chat_message(&self, session_id: &str, role: &str, content: &str) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO chat_history (session_id, role, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, role, content],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    }

    pub fn load_chat_history(&self, session_id: &str) -> Result<Vec<(String, String)>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content FROM chat_history WHERE session_id = ?1 ORDER BY id ASC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| Error::Db(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| Error::Db(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn save_input_history(&self, session_id: &str, command: &str) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO input_history (session_id, command) VALUES (?1, ?2)",
            rusqlite::params![session_id, command],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn save_command_result(
        &self,
        session_id: &str,
        command: &str,
        exit_code: i32,
        duration_ms: i64,
        output_preview: &str,
    ) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO command_history (session_id, command, exit_code, duration_ms, output_preview)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, command, exit_code, duration_ms, output_preview],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn link_input_to_command(&self, input_history_id: i64, command_history_id: i64) -> Result<(), Error> {
        self.conn.execute(
            "UPDATE input_history SET command_history_id = ?1 WHERE id = ?2",
            rusqlite::params![command_history_id, input_history_id],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    }

    pub fn get_command_result(&self, command_history_id: i64) -> Result<Option<CommandResult>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT command, exit_code, duration_ms, output_preview, started_at
             FROM command_history WHERE id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let result = stmt.query_row(rusqlite::params![command_history_id], |row| {
            Ok(CommandResult {
                command: row.get(0)?,
                exit_code: row.get(1)?,
                duration_ms: row.get(2)?,
                output_preview: row.get(3)?,
                started_at: row.get(4)?,
            })
        });
        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Db(e.to_string())),
        }
    }

    pub fn load_input_history(&self, session_id: &str) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT command FROM input_history WHERE session_id = ?1 ORDER BY id ASC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| Error::Db(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| Error::Db(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn load_recent_commands(&self, session_id: &str, limit: usize) -> Result<Vec<(String, Option<i64>)>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT command, command_history_id FROM input_history
             WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![session_id, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
        }).map_err(|e| Error::Db(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| Error::Db(e.to_string()))?);
        }
        result.reverse();
        Ok(result)
    }
}
