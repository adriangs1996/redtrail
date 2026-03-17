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

    pub fn add_host(&self, session_id: &str, ip: &str, os: Option<&str>, hostname: Option<&str>) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT OR IGNORE INTO hosts (session_id, ip, os, hostname) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, ip, os, hostname],
        ).map_err(|e| Error::Db(e.to_string()))?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM hosts WHERE session_id = ?1 AND ip = ?2",
            params![session_id, ip],
            |r| r.get(0),
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(id)
    }

    pub fn add_port(&self, session_id: &str, host_ip: &str, port: i64, protocol: Option<&str>, service: Option<&str>, version: Option<&str>) -> Result<i64, Error> {
        let host_id = self.add_host(session_id, host_ip, None, None)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session_id, host_id, port, protocol.unwrap_or("tcp"), service, version],
        ).map_err(|e| Error::Db(e.to_string()))?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM ports WHERE host_id = ?1 AND port = ?2 AND protocol = ?3",
            params![host_id, port, protocol.unwrap_or("tcp")],
            |r| r.get(0),
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(id)
    }

    pub fn add_credential(&self, session_id: &str, username: &str, password: Option<&str>, hash: Option<&str>, service: Option<&str>, host: Option<&str>, source: Option<&str>) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO credentials (session_id, username, password, hash, service, host, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session_id, username, password, hash, service, host, source],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_flag(&self, session_id: &str, value: &str, source: Option<&str>) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO flags (session_id, value, source) VALUES (?1, ?2, ?3)",
            params![session_id, value, source],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_access(&self, session_id: &str, host: &str, user: &str, level: &str, method: Option<&str>) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO access_levels (session_id, host, user, level, method) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, host, user, level, method],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_note(&self, session_id: &str, text: &str) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO notes (session_id, text) VALUES (?1, ?2)",
            params![session_id, text],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_command(&self, session_id: &str, command: &str, tool: Option<&str>) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO command_history (session_id, command, tool) VALUES (?1, ?2, ?3)",
            params![session_id, command, tool],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn decrement_noise_budget(&self, session_id: &str, cost: f64) -> Result<(), Error> {
        self.conn.execute(
            "UPDATE sessions SET noise_budget = max(0, noise_budget - ?1) WHERE id = ?2",
            rusqlite::params![cost, session_id],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    }

    pub fn finish_command(&self, id: i64, exit_code: i32, duration_ms: i64, output: &str) -> Result<(), Error> {
        let preview = if output.len() > 500 { &output[..500] } else { output };
        self.conn.execute(
            "UPDATE command_history SET exit_code = ?1, duration_ms = ?2, output = ?3, output_preview = ?4 WHERE id = ?5",
            params![exit_code, duration_ms, output, preview, id],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    }

    pub fn list_hosts(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT ip, hostname, os, status FROM hosts WHERE session_id = ?1 ORDER BY ip"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "ip": r.get::<_, String>(0)?,
                "hostname": r.get::<_, Option<String>>(1)?,
                "os": r.get::<_, Option<String>>(2)?,
                "status": r.get::<_, String>(3)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?;
        rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
    }

    pub fn list_ports(&self, session_id: &str, host_filter: Option<&str>) -> Result<Vec<serde_json::Value>, Error> {
        let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
            Ok(serde_json::json!({
                "ip": r.get::<_, String>(0)?,
                "port": r.get::<_, i64>(1)?,
                "protocol": r.get::<_, String>(2)?,
                "service": r.get::<_, Option<String>>(3)?,
                "version": r.get::<_, Option<String>>(4)?,
            }))
        };
        if let Some(ip) = host_filter {
            let mut stmt = self.conn.prepare(
                "SELECT h.ip, p.port, p.protocol, p.service, p.version FROM ports p JOIN hosts h ON p.host_id = h.id WHERE p.session_id = ?1 AND h.ip = ?2 ORDER BY p.port"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, ip], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT h.ip, p.port, p.protocol, p.service, p.version FROM ports p JOIN hosts h ON p.host_id = h.id WHERE p.session_id = ?1 ORDER BY p.port"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
    }

    pub fn list_credentials(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT username, password, hash, service, host, source FROM credentials WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "username": r.get::<_, String>(0)?,
                "password": r.get::<_, Option<String>>(1)?,
                "hash": r.get::<_, Option<String>>(2)?,
                "service": r.get::<_, Option<String>>(3)?,
                "host": r.get::<_, Option<String>>(4)?,
                "source": r.get::<_, Option<String>>(5)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?;
        rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
    }

    pub fn list_flags(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT value, source, captured_at FROM flags WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "value": r.get::<_, String>(0)?,
                "source": r.get::<_, Option<String>>(1)?,
                "captured_at": r.get::<_, String>(2)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?;
        rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
    }

    pub fn list_access(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT host, user, level, method FROM access_levels WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "host": r.get::<_, String>(0)?,
                "user": r.get::<_, String>(1)?,
                "level": r.get::<_, String>(2)?,
                "method": r.get::<_, Option<String>>(3)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?;
        rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
    }

    pub fn list_notes(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT text, created_at FROM notes WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "text": r.get::<_, String>(0)?,
                "created_at": r.get::<_, String>(1)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?;
        rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
    }

    pub fn list_history(&self, session_id: &str, limit: usize) -> Result<Vec<serde_json::Value>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, command, exit_code, duration_ms, tool, started_at FROM command_history WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, limit as i64], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "command": r.get::<_, String>(1)?,
                "exit_code": r.get::<_, Option<i64>>(2)?,
                "duration_ms": r.get::<_, Option<i64>>(3)?,
                "tool": r.get::<_, Option<String>>(4)?,
                "started_at": r.get::<_, String>(5)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?;
        rows.map(|r| r.map_err(|e| Error::Db(e.to_string()))).collect()
    }

    pub fn search(&self, session_id: &str, query: &str) -> Result<Vec<serde_json::Value>, Error> {
        let pattern = format!("%{query}%");
        let mut results: Vec<serde_json::Value> = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT 'host' as kind, ip as value FROM hosts WHERE session_id = ?1 AND (ip LIKE ?2 OR hostname LIKE ?2)"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, &pattern], |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        }).map_err(|e| Error::Db(e.to_string()))?;
        for r in rows { results.push(r.map_err(|e| Error::Db(e.to_string()))?); }

        let mut stmt = self.conn.prepare(
            "SELECT 'credential' as kind, username as value FROM credentials WHERE session_id = ?1 AND username LIKE ?2"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, &pattern], |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        }).map_err(|e| Error::Db(e.to_string()))?;
        for r in rows { results.push(r.map_err(|e| Error::Db(e.to_string()))?); }

        let mut stmt = self.conn.prepare(
            "SELECT 'note' as kind, text as value FROM notes WHERE session_id = ?1 AND text LIKE ?2"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, &pattern], |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        }).map_err(|e| Error::Db(e.to_string()))?;
        for r in rows { results.push(r.map_err(|e| Error::Db(e.to_string()))?); }

        let mut stmt = self.conn.prepare(
            "SELECT 'command' as kind, command as value FROM command_history WHERE session_id = ?1 AND command LIKE ?2"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id, &pattern], |r| {
            Ok(serde_json::json!({"kind": r.get::<_, String>(0)?, "value": r.get::<_, String>(1)?}))
        }).map_err(|e| Error::Db(e.to_string()))?;
        for r in rows { results.push(r.map_err(|e| Error::Db(e.to_string()))?); }

        Ok(results)
    }

    pub fn create_hypothesis(
        &self,
        session_id: &str,
        statement: &str,
        category: &str,
        priority: &str,
        confidence: f64,
        target_component: Option<&str>,
    ) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, priority, confidence, target_component)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session_id, statement, category, priority, confidence, target_component],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_hypotheses(
        &self,
        session_id: &str,
        status_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        if let Some(status) = status_filter {
            let mut stmt = self.conn.prepare(
                "SELECT id, statement, category, status, priority, confidence, target_component, created_at
                 FROM hypotheses WHERE session_id = ?1 AND status = ?2 ORDER BY created_at DESC"
            ).map_err(|e| Error::Db(e.to_string()))?;
            let rows = stmt.query_map(params![session_id, status], map_hypothesis_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| Error::Db(e.to_string()))?;
            return Ok(rows);
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, statement, category, status, priority, confidence, target_component, created_at
             FROM hypotheses WHERE session_id = ?1 ORDER BY created_at DESC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt.query_map(params![session_id], map_hypothesis_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(rows)
    }

    pub fn update_hypothesis(&self, id: i64, status: &str) -> Result<(), Error> {
        if status == "confirmed" || status == "refuted" {
            self.conn.execute(
                "UPDATE hypotheses SET status = ?1, resolved_at = datetime('now') WHERE id = ?2",
                params![status, id],
            ).map_err(|e| Error::Db(e.to_string()))?;
        } else {
            self.conn.execute(
                "UPDATE hypotheses SET status = ?1 WHERE id = ?2",
                params![status, id],
            ).map_err(|e| Error::Db(e.to_string()))?;
        }
        Ok(())
    }

    pub fn get_hypothesis(&self, id: i64) -> Result<serde_json::Value, Error> {
        let mut hyp = self.conn.query_row(
            "SELECT id, statement, category, status, priority, confidence, target_component, created_at, resolved_at
             FROM hypotheses WHERE id = ?1",
            params![id],
            |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "statement": r.get::<_, String>(1)?,
                    "category": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?,
                    "priority": r.get::<_, String>(4)?,
                    "confidence": r.get::<_, f64>(5)?,
                    "target_component": r.get::<_, Option<String>>(6)?,
                    "created_at": r.get::<_, String>(7)?,
                    "resolved_at": r.get::<_, Option<String>>(8)?,
                }))
            },
        ).map_err(|e| Error::Db(e.to_string()))?;

        let mut stmt = self.conn.prepare(
            "SELECT id, finding, severity, poc, created_at FROM evidence WHERE hypothesis_id = ?1 ORDER BY created_at DESC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let evidence: Vec<serde_json::Value> = stmt.query_map(params![id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "finding": r.get::<_, String>(1)?,
                "severity": r.get::<_, String>(2)?,
                "poc": r.get::<_, Option<String>>(3)?,
                "created_at": r.get::<_, String>(4)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

        hyp["evidence"] = serde_json::Value::Array(evidence);
        Ok(hyp)
    }

    pub fn create_evidence(
        &self,
        session_id: &str,
        hypothesis_id: Option<i64>,
        finding: &str,
        severity: &str,
        poc: Option<&str>,
    ) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO evidence (session_id, hypothesis_id, finding, severity, poc)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, hypothesis_id, finding, severity, poc],
        ).map_err(|e| Error::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_evidence(
        &self,
        session_id: &str,
        hypothesis_id: Option<i64>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        if let Some(hid) = hypothesis_id {
            let mut stmt = self.conn.prepare(
                "SELECT id, hypothesis_id, finding, severity, poc, created_at
                 FROM evidence WHERE session_id = ?1 AND hypothesis_id = ?2 ORDER BY created_at DESC"
            ).map_err(|e| Error::Db(e.to_string()))?;
            return stmt.query_map(params![session_id, hid], map_evidence_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| Error::Db(e.to_string()));
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, hypothesis_id, finding, severity, poc, created_at
             FROM evidence WHERE session_id = ?1 ORDER BY created_at DESC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id], map_evidence_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))
    }

    pub fn export_evidence(&self, session_id: &str) -> Result<Vec<serde_json::Value>, Error> {
        let mut hyp_stmt = self.conn.prepare(
            "SELECT id, statement, category, status FROM hypotheses WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let hypotheses: Vec<(i64, String, String, String)> = hyp_stmt.query_map(
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

        let mut result = Vec::new();

        for (hid, statement, category, status) in hypotheses {
            let mut ev_stmt = self.conn.prepare(
                "SELECT id, finding, severity, poc, created_at FROM evidence WHERE hypothesis_id = ?1"
            ).map_err(|e| Error::Db(e.to_string()))?;
            let evidence: Vec<serde_json::Value> = ev_stmt.query_map(params![hid], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "finding": r.get::<_, String>(1)?,
                    "severity": r.get::<_, String>(2)?,
                    "poc": r.get::<_, Option<String>>(3)?,
                    "created_at": r.get::<_, String>(4)?,
                }))
            }).map_err(|e| Error::Db(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;

            result.push(serde_json::json!({
                "hypothesis_id": hid,
                "statement": statement,
                "category": category,
                "status": status,
                "evidence": evidence,
            }));
        }

        let mut orphan_stmt = self.conn.prepare(
            "SELECT id, finding, severity, poc, created_at FROM evidence WHERE session_id = ?1 AND hypothesis_id IS NULL"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let orphans: Vec<serde_json::Value> = orphan_stmt.query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "finding": r.get::<_, String>(1)?,
                "severity": r.get::<_, String>(2)?,
                "poc": r.get::<_, Option<String>>(3)?,
                "created_at": r.get::<_, String>(4)?,
            }))
        }).map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

        if !orphans.is_empty() {
            result.push(serde_json::json!({
                "hypothesis_id": null,
                "statement": null,
                "category": null,
                "status": null,
                "evidence": orphans,
            }));
        }

        Ok(result)
    }

    pub fn status_summary(&self, session_id: &str) -> Result<serde_json::Value, Error> {
        let (name, target, goal, phase, noise_budget): (String, Option<String>, String, String, f64) = self.conn.query_row(
            "SELECT name, target, goal, phase, noise_budget FROM sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        ).map_err(|e| Error::Db(e.to_string()))?;

        let hosts: i64 = self.conn.query_row("SELECT count(*) FROM hosts WHERE session_id = ?1", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let ports: i64 = self.conn.query_row("SELECT count(*) FROM ports WHERE session_id = ?1", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let creds: i64 = self.conn.query_row("SELECT count(*) FROM credentials WHERE session_id = ?1", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let flags: i64 = self.conn.query_row("SELECT count(*) FROM flags WHERE session_id = ?1", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let access: i64 = self.conn.query_row("SELECT count(*) FROM access_levels WHERE session_id = ?1", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let hyps_pending: i64 = self.conn.query_row("SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'pending'", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let hyps_confirmed: i64 = self.conn.query_row("SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'confirmed'", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;
        let hyps_refuted: i64 = self.conn.query_row("SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'refuted'", params![session_id], |r| r.get(0)).map_err(|e| Error::Db(e.to_string()))?;

        Ok(serde_json::json!({
            "session_name": name,
            "target": target,
            "goal": goal,
            "phase": phase,
            "hosts": hosts,
            "ports": ports,
            "creds": creds,
            "flags": flags,
            "access": access,
            "hypotheses_pending": hyps_pending,
            "hypotheses_confirmed": hyps_confirmed,
            "hypotheses_refuted": hyps_refuted,
            "noise_budget": noise_budget,
        }))
    }
}

fn map_hypothesis_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": r.get::<_, i64>(0)?,
        "statement": r.get::<_, String>(1)?,
        "category": r.get::<_, String>(2)?,
        "status": r.get::<_, String>(3)?,
        "priority": r.get::<_, String>(4)?,
        "confidence": r.get::<_, f64>(5)?,
        "target_component": r.get::<_, Option<String>>(6)?,
        "created_at": r.get::<_, String>(7)?,
    }))
}

fn map_evidence_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": r.get::<_, i64>(0)?,
        "hypothesis_id": r.get::<_, Option<i64>>(1)?,
        "finding": r.get::<_, String>(2)?,
        "severity": r.get::<_, String>(3)?,
        "poc": r.get::<_, Option<String>>(4)?,
        "created_at": r.get::<_, String>(5)?,
    }))
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
