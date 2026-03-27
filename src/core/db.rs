use crate::error::Error;
use rusqlite::Connection;

const SCHEMA: &str = "
-- Raw command capture
CREATE TABLE IF NOT EXISTS commands (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    timestamp_start INTEGER NOT NULL,
    timestamp_end INTEGER,
    command_raw TEXT NOT NULL,
    command_binary TEXT,
    command_subcommand TEXT,
    command_args TEXT,
    command_flags TEXT,
    cwd TEXT,
    git_repo TEXT,
    git_branch TEXT,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    stdout_truncated BOOLEAN DEFAULT 0,
    stderr_truncated BOOLEAN DEFAULT 0,
    env_snapshot TEXT,
    hostname TEXT,
    shell TEXT,
    source TEXT NOT NULL DEFAULT 'human',
    agent_session_id TEXT,
    parent_process TEXT,
    is_automated BOOLEAN DEFAULT 0,
    redacted BOOLEAN DEFAULT 0,
    extracted BOOLEAN DEFAULT 0,
    created_at INTEGER DEFAULT (unixepoch())
);

-- Terminal sessions
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    started_at INTEGER,
    ended_at INTEGER,
    cwd_initial TEXT,
    hostname TEXT,
    shell TEXT,
    source TEXT NOT NULL DEFAULT 'human',
    agent_session_id TEXT,
    command_count INTEGER DEFAULT 0,
    error_count INTEGER DEFAULT 0,
    human_command_count INTEGER DEFAULT 0,
    agent_command_count INTEGER DEFAULT 0,
    summary TEXT
);

-- Extracted entities
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    properties TEXT,
    first_seen INTEGER,
    last_seen INTEGER,
    source_command_id TEXT,
    FOREIGN KEY (source_command_id) REFERENCES commands(id)
);

-- Entity relationships
CREATE TABLE IF NOT EXISTS relationships (
    id TEXT PRIMARY KEY,
    source_entity_id TEXT NOT NULL,
    target_entity_id TEXT NOT NULL,
    type TEXT NOT NULL,
    properties TEXT,
    observed_at INTEGER,
    source_command_id TEXT,
    FOREIGN KEY (source_entity_id) REFERENCES entities(id),
    FOREIGN KEY (target_entity_id) REFERENCES entities(id)
);

-- Behavioral patterns
CREATE TABLE IF NOT EXISTS patterns (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    description TEXT,
    trigger_signature TEXT,
    command_sequence TEXT,
    frequency INTEGER,
    confidence REAL,
    last_observed INTEGER,
    first_observed INTEGER,
    active BOOLEAN DEFAULT 1
);

-- Error resolution mappings
CREATE TABLE IF NOT EXISTS error_resolutions (
    id TEXT PRIMARY KEY,
    error_signature TEXT NOT NULL,
    error_domain TEXT,
    resolution_commands TEXT,
    success_rate REAL,
    occurrences INTEGER,
    avg_time_to_resolve INTEGER,
    last_seen INTEGER
);

-- Proactive suggestions
CREATE TABLE IF NOT EXISTS suggestions (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    trigger_condition TEXT,
    message TEXT,
    source_pattern_id TEXT,
    priority INTEGER,
    shown_count INTEGER DEFAULT 0,
    dismissed_count INTEGER DEFAULT 0,
    accepted_count INTEGER DEFAULT 0,
    active BOOLEAN DEFAULT 1,
    FOREIGN KEY (source_pattern_id) REFERENCES patterns(id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_commands_binary_ts ON commands(command_binary, timestamp_start);
CREATE INDEX IF NOT EXISTS idx_commands_session_ts ON commands(session_id, timestamp_start);
CREATE INDEX IF NOT EXISTS idx_commands_cwd_ts ON commands(cwd, timestamp_start);
CREATE INDEX IF NOT EXISTS idx_commands_exit ON commands(exit_code) WHERE exit_code != 0;
CREATE INDEX IF NOT EXISTS idx_commands_extracted ON commands(extracted) WHERE extracted = 0;
CREATE INDEX IF NOT EXISTS idx_entities_type_name ON entities(type, name);
CREATE INDEX IF NOT EXISTS idx_entities_last_seen ON entities(last_seen);
CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_entity_id);
CREATE INDEX IF NOT EXISTS idx_patterns_type ON patterns(type, active);

-- Full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS commands_fts USING fts5(
    command_raw, stdout, stderr,
    content='commands', content_rowid='rowid'
);
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
    set_file_permissions(path);
    Ok(conn)
}

#[cfg(unix)]
fn set_file_permissions(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &str) {}

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

// --- Command insert/query API ---

#[derive(Default, Clone, Copy)]
pub struct NewCommand<'a> {
    pub session_id: &'a str,
    pub command_raw: &'a str,
    pub command_binary: Option<&'a str>,
    pub command_subcommand: Option<&'a str>,
    pub command_args: Option<&'a str>,
    pub command_flags: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub git_repo: Option<&'a str>,
    pub git_branch: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub stdout: Option<&'a str>,
    pub stderr: Option<&'a str>,
    pub env_snapshot: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub source: &'a str,
    pub timestamp_start: i64,
    pub timestamp_end: Option<i64>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub redacted: bool,
}

pub struct CommandRow {
    pub id: String,
    pub session_id: String,
    pub command_raw: String,
    pub command_binary: Option<String>,
    pub cwd: Option<String>,
    pub exit_code: Option<i32>,
    pub hostname: Option<String>,
    pub shell: Option<String>,
    pub source: String,
    pub timestamp_start: i64,
    pub timestamp_end: Option<i64>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub redacted: bool,
}

#[derive(Default)]
pub struct CommandFilter<'a> {
    pub failed_only: bool,
    pub command_binary: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub since: Option<i64>,
    pub limit: Option<usize>,
}

pub fn insert_command(conn: &Connection, cmd: &NewCommand) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, timestamp_end, command_raw, command_binary, command_subcommand, command_args, command_flags, cwd, git_repo, git_branch, exit_code, stdout, stderr, stdout_truncated, stderr_truncated, env_snapshot, hostname, shell, source, redacted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
        rusqlite::params![
            id, cmd.session_id, cmd.timestamp_start, cmd.timestamp_end,
            cmd.command_raw, cmd.command_binary, cmd.command_subcommand,
            cmd.command_args, cmd.command_flags,
            cmd.cwd, cmd.git_repo, cmd.git_branch, cmd.exit_code,
            cmd.stdout, cmd.stderr, cmd.stdout_truncated, cmd.stderr_truncated,
            cmd.env_snapshot, cmd.hostname, cmd.shell, cmd.source, cmd.redacted,
        ],
    ).map_err(|e| Error::Db(e.to_string()))?;

    // Sync FTS index
    let rowid: i64 = conn
        .query_row("SELECT rowid FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO commands_fts(rowid, command_raw, stdout, stderr) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![rowid, cmd.command_raw, cmd.stdout, cmd.stderr],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Update session counters
    conn.execute(
        "UPDATE sessions SET command_count = command_count + 1 WHERE id = ?1",
        [cmd.session_id],
    )
    .ok(); // Best-effort: session may not exist (e.g. test without session setup)

    if cmd.exit_code.is_some_and(|c| c != 0) {
        conn.execute(
            "UPDATE sessions SET error_count = error_count + 1 WHERE id = ?1",
            [cmd.session_id],
        )
        .ok();
    }

    Ok(id)
}

pub fn get_commands(conn: &Connection, filter: &CommandFilter) -> Result<Vec<CommandRow>, Error> {
    let mut sql = String::from(
        "SELECT id, session_id, command_raw, command_binary, cwd, exit_code, hostname, shell, source, timestamp_start, timestamp_end, stdout, stderr, stdout_truncated, stderr_truncated, redacted FROM commands WHERE 1=1"
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if filter.failed_only {
        sql.push_str(" AND exit_code IS NOT NULL AND exit_code != 0");
    }
    if let Some(bin) = filter.command_binary {
        sql.push_str(&format!(" AND command_binary = ?{idx}"));
        params.push(Box::new(bin.to_string()));
        idx += 1;
    }
    if let Some(cwd) = filter.cwd {
        sql.push_str(&format!(" AND cwd = ?{idx}"));
        params.push(Box::new(cwd.to_string()));
        idx += 1;
    }
    if let Some(sid) = filter.session_id {
        sql.push_str(&format!(" AND session_id = ?{idx}"));
        params.push(Box::new(sid.to_string()));
        idx += 1;
    }
    if let Some(since) = filter.since {
        sql.push_str(&format!(" AND timestamp_start >= ?{idx}"));
        params.push(Box::new(since));
        #[allow(unused_assignments)]
        { idx += 1; }
    }

    sql.push_str(" ORDER BY timestamp_start DESC");

    let limit = filter.limit.unwrap_or(50);
    sql.push_str(&format!(" LIMIT {limit}"));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(CommandRow {
            id: row.get(0)?,
            session_id: row.get(1)?,
            command_raw: row.get(2)?,
            command_binary: row.get(3)?,
            cwd: row.get(4)?,
            exit_code: row.get(5)?,
            hostname: row.get(6)?,
            shell: row.get(7)?,
            source: row.get(8)?,
            timestamp_start: row.get(9)?,
            timestamp_end: row.get(10)?,
            stdout: row.get(11)?,
            stderr: row.get(12)?,
            stdout_truncated: row.get(13)?,
            stderr_truncated: row.get(14)?,
            redacted: row.get(15)?,
        })
    }).map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Insert a command with automatic secret redaction on command_raw, stdout, and stderr.
pub fn insert_command_redacted(conn: &Connection, cmd: &NewCommand) -> Result<String, Error> {
    use crate::core::secrets::engine::redact_secrets;

    let redacted_raw = redact_secrets(cmd.command_raw);
    let redacted_stdout = cmd.stdout.map(redact_secrets);
    let redacted_stderr = cmd.stderr.map(redact_secrets);

    let was_redacted = redacted_raw != cmd.command_raw
        || cmd.stdout.is_some_and(|s| redacted_stdout.as_deref() != Some(s))
        || cmd.stderr.is_some_and(|s| redacted_stderr.as_deref() != Some(s));

    let redacted_cmd = NewCommand {
        command_raw: &redacted_raw,
        stdout: redacted_stdout.as_deref(),
        stderr: redacted_stderr.as_deref(),
        redacted: was_redacted,
        ..*cmd
    };

    insert_command(conn, &redacted_cmd)
}

// --- Session API ---

#[derive(Default)]
pub struct NewSession<'a> {
    pub cwd_initial: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub source: &'a str,
}

pub struct SessionRow {
    pub id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub cwd_initial: Option<String>,
    pub hostname: Option<String>,
    pub shell: Option<String>,
    pub source: String,
    pub command_count: i64,
    pub error_count: i64,
}

pub fn create_session(conn: &Connection, s: &NewSession) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO sessions (id, started_at, cwd_initial, hostname, shell, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, now, s.cwd_initial, s.hostname, s.shell, s.source],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(id)
}

pub fn get_session(conn: &Connection, id: &str) -> Result<SessionRow, Error> {
    conn.query_row(
        "SELECT id, started_at, ended_at, cwd_initial, hostname, shell, source, command_count, error_count
         FROM sessions WHERE id = ?1",
        [id],
        |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                cwd_initial: row.get(3)?,
                hostname: row.get(4)?,
                shell: row.get(5)?,
                source: row.get(6)?,
                command_count: row.get(7)?,
                error_count: row.get(8)?,
            })
        },
    )
    .map_err(|e| Error::Db(e.to_string()))
}

pub fn list_sessions(conn: &Connection, limit: usize) -> Result<Vec<SessionRow>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT id, started_at, ended_at, cwd_initial, hostname, shell, source, command_count, error_count
             FROM sessions ORDER BY started_at DESC LIMIT ?1",
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                cwd_initial: row.get(3)?,
                hostname: row.get(4)?,
                shell: row.get(5)?,
                source: row.get(6)?,
                command_count: row.get(7)?,
                error_count: row.get(8)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

// --- Full-text search ---

pub fn search_commands(conn: &Connection, query: &str, limit: usize) -> Result<Vec<CommandRow>, Error> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.session_id, c.command_raw, c.command_binary, c.cwd, c.exit_code, c.hostname, c.shell, c.source, c.timestamp_start, c.timestamp_end, c.stdout, c.stderr, c.stdout_truncated, c.stderr_truncated, c.redacted
         FROM commands_fts fts
         JOIN commands c ON c.rowid = fts.rowid
         WHERE commands_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2"
    ).map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
        Ok(CommandRow {
            id: row.get(0)?,
            session_id: row.get(1)?,
            command_raw: row.get(2)?,
            command_binary: row.get(3)?,
            cwd: row.get(4)?,
            exit_code: row.get(5)?,
            hostname: row.get(6)?,
            shell: row.get(7)?,
            source: row.get(8)?,
            timestamp_start: row.get(9)?,
            timestamp_end: row.get(10)?,
            stdout: row.get(11)?,
            stderr: row.get(12)?,
            stdout_truncated: row.get(13)?,
            stderr_truncated: row.get(14)?,
            redacted: row.get(15)?,
        })
    }).map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

// --- Forget / delete API ---

pub fn forget_command(conn: &Connection, id: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO commands_fts(commands_fts, rowid, command_raw, stdout, stderr)
         SELECT 'delete', rowid, command_raw, stdout, stderr FROM commands WHERE id = ?1",
        [id],
    ).map_err(|e| Error::Db(e.to_string()))?;

    conn.execute("DELETE FROM commands WHERE id = ?1", [id])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn forget_session(conn: &Connection, session_id: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO commands_fts(commands_fts, rowid, command_raw, stdout, stderr)
         SELECT 'delete', rowid, command_raw, stdout, stderr FROM commands WHERE session_id = ?1",
        [session_id],
    ).map_err(|e| Error::Db(e.to_string()))?;

    conn.execute("DELETE FROM commands WHERE session_id = ?1", [session_id])
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn forget_since(conn: &Connection, since_ts: i64) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO commands_fts(commands_fts, rowid, command_raw, stdout, stderr)
         SELECT 'delete', rowid, command_raw, stdout, stderr FROM commands WHERE timestamp_start >= ?1",
        [since_ts],
    ).map_err(|e| Error::Db(e.to_string()))?;

    conn.execute("DELETE FROM commands WHERE timestamp_start >= ?1", [since_ts])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}
