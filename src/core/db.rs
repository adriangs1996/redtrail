use crate::core::extractor::{Fact, Relation};
use crate::error::Error;
use rusqlite::Connection;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    workspace_path TEXT NOT NULL UNIQUE,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    command TEXT NOT NULL,
    tool TEXT,
    exit_code INTEGER,
    duration_ms INTEGER,
    output TEXT,
    output_hash TEXT,
    extraction_status TEXT DEFAULT 'stored',
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS facts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    event_id INTEGER NOT NULL REFERENCES events(id),
    fact_type TEXT NOT NULL,
    key TEXT NOT NULL,
    attributes TEXT NOT NULL DEFAULT '{}',
    confidence REAL NOT NULL DEFAULT 1.0,
    source TEXT NOT NULL DEFAULT 'regex',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, key)
);

CREATE TABLE IF NOT EXISTS relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    from_key TEXT NOT NULL,
    to_key TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, from_key, to_key, relation_type)
);

CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_tool ON events(tool);
CREATE INDEX IF NOT EXISTS idx_facts_session_type ON facts(session_id, fact_type);
CREATE INDEX IF NOT EXISTS idx_facts_event ON facts(event_id);
CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(session_id, from_key);
CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(session_id, to_key);
";

fn init(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn open(path: &str) -> Result<Connection, Error> {
    let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
    init(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection, Error> {
    let conn = Connection::open_in_memory().map_err(|e| Error::Db(e.to_string()))?;
    init(&conn)?;
    Ok(conn)
}

pub fn global_db_path() -> Result<std::path::PathBuf, Error> {
    let home = std::env::var("HOME").map_err(|_| Error::Config("HOME not set".into()))?;
    let dir = std::path::PathBuf::from(home).join(".redtrail");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("redtrail-v2.db"))
}

pub fn ensure_session(conn: &Connection, workspace_path: &str) -> Result<String, Error> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM sessions WHERE workspace_path = ?1",
            [workspace_path],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    let dir_name = std::path::Path::new(workspace_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("session");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let id = format!("{dir_name}-{ts}");

    conn.execute(
        "INSERT INTO sessions (id, name, workspace_path) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, dir_name, workspace_path],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(id)
}

pub fn insert_event(
    conn: &Connection,
    session_id: &str,
    command: &str,
    tool: Option<&str>,
    exit_code: i32,
    duration_ms: i64,
    output: &str,
    output_hash: &str,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO events (session_id, command, tool, exit_code, duration_ms, output, output_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![session_id, command, tool, exit_code, duration_ms, output, output_hash],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn update_extraction_status(
    conn: &Connection,
    event_id: i64,
    status: &str,
) -> Result<(), Error> {
    conn.execute(
        "UPDATE events SET extraction_status = ?1 WHERE id = ?2",
        rusqlite::params![status, event_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn insert_fact(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
    fact_type: &str,
    key: &str,
    attributes: &serde_json::Value,
    confidence: f64,
    source: &str,
) -> Result<i64, Error> {
    let attr_str = serde_json::to_string(attributes).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO facts (session_id, event_id, fact_type, key, attributes, confidence, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(session_id, key) DO UPDATE SET
            attributes = json_patch(facts.attributes, excluded.attributes),
            confidence = MAX(facts.confidence, excluded.confidence),
            source = CASE WHEN excluded.confidence > facts.confidence THEN excluded.source ELSE facts.source END,
            event_id = excluded.event_id,
            updated_at = datetime('now')",
        rusqlite::params![session_id, event_id, fact_type, key, attr_str, confidence, source],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_relation(
    conn: &Connection,
    session_id: &str,
    from_key: &str,
    to_key: &str,
    relation_type: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT OR IGNORE INTO relations (session_id, from_key, to_key, relation_type)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session_id, from_key, to_key, relation_type],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn store_extraction(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
    facts: &[Fact],
    relations: &[Relation],
) -> Result<(), Error> {
    let tx = conn.unchecked_transaction().map_err(|e| Error::Db(e.to_string()))?;

    for fact in facts {
        insert_fact(&tx, session_id, event_id, &fact.fact_type, &fact.key, &fact.attributes, 1.0, "regex")?;
    }
    for rel in relations {
        insert_relation(&tx, session_id, &rel.from_key, &rel.to_key, &rel.relation_type)?;
    }
    update_extraction_status(&tx, event_id, "extracted")?;

    tx.commit().map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}
