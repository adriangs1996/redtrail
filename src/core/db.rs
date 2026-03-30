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
    stdout_compressed BLOB,
    stderr_compressed BLOB,
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
    created_at INTEGER DEFAULT (unixepoch()),
    tool_name TEXT,
    tool_input TEXT,
    tool_response TEXT
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
CREATE INDEX IF NOT EXISTS idx_commands_tool ON commands(tool_name) WHERE tool_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_commands_ts ON commands(timestamp_start);
CREATE INDEX IF NOT EXISTS idx_commands_source ON commands(source, timestamp_start);
CREATE INDEX IF NOT EXISTS idx_commands_agent_session ON commands(agent_session_id, timestamp_start) WHERE agent_session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_commands_git_repo ON commands(git_repo, timestamp_start) WHERE git_repo IS NOT NULL;

-- Redaction audit log
CREATE TABLE IF NOT EXISTS redaction_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command_id TEXT,
    field TEXT NOT NULL,
    pattern_label TEXT NOT NULL,
    redacted_at INTEGER DEFAULT (unixepoch()),
    FOREIGN KEY (command_id) REFERENCES commands(id)
);
CREATE INDEX IF NOT EXISTS idx_redaction_log_cmd ON redaction_log(command_id);

-- Full-text search (external content — we manage inserts/deletes manually)
CREATE VIRTUAL TABLE IF NOT EXISTS commands_fts USING fts5(
    command_raw, stdout, stderr
);
";

fn init(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA busy_timeout=3000;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    migrate_agent_columns(conn)?;
    migrate_compressed_columns(conn)?;
    conn.execute_batch("PRAGMA optimize;")
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Add tool_name, tool_input, tool_response columns if missing (for existing databases).
fn migrate_agent_columns(conn: &Connection) -> Result<(), Error> {
    let has_tool_name: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .map_err(|e| Error::Db(e.to_string()))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .any(|col| col.as_deref() == Ok("tool_name"));

    if !has_tool_name {
        conn.execute_batch(
            "ALTER TABLE commands ADD COLUMN tool_name TEXT;
             ALTER TABLE commands ADD COLUMN tool_input TEXT;
             ALTER TABLE commands ADD COLUMN tool_response TEXT;
             CREATE INDEX IF NOT EXISTS idx_commands_tool ON commands(tool_name) WHERE tool_name IS NOT NULL;"
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    }
    Ok(())
}

/// Add stdout_compressed, stderr_compressed columns if missing (for existing databases).
fn migrate_compressed_columns(conn: &Connection) -> Result<(), Error> {
    let has_col: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .map_err(|e| Error::Db(e.to_string()))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .any(|col| col.as_deref() == Ok("stdout_compressed"));

    if !has_col {
        conn.execute_batch(
            "ALTER TABLE commands ADD COLUMN stdout_compressed BLOB;
             ALTER TABLE commands ADD COLUMN stderr_compressed BLOB;"
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    }
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
    pub source: Option<&'a str>,
    pub tool_name: Option<&'a str>,
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

fn decompress_blob(blob: &[u8]) -> Option<String> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(blob);
    let mut out = String::new();
    decoder.read_to_string(&mut out).ok()?;
    Some(out)
}

pub fn get_commands(conn: &Connection, filter: &CommandFilter) -> Result<Vec<CommandRow>, Error> {
    let mut sql = String::from(
        "SELECT id, session_id, command_raw, command_binary, cwd, exit_code, hostname, shell, source, timestamp_start, timestamp_end, stdout, stderr, stdout_truncated, stderr_truncated, redacted, stdout_compressed, stderr_compressed FROM commands WHERE 1=1"
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
        idx += 1;
    }
    if let Some(source) = filter.source {
        sql.push_str(&format!(" AND source = ?{idx}"));
        params.push(Box::new(source.to_string()));
        idx += 1;
    }
    if let Some(tool) = filter.tool_name {
        sql.push_str(&format!(" AND tool_name = ?{idx}"));
        params.push(Box::new(tool.to_string()));
        #[allow(unused_assignments)]
        { idx += 1; }
    }

    sql.push_str(" ORDER BY timestamp_start DESC");

    let limit = filter.limit.unwrap_or(50);
    sql.push_str(&format!(" LIMIT {limit}"));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let stdout_text: Option<String> = row.get(11)?;
        let stderr_text: Option<String> = row.get(12)?;
        let stdout_compressed: Option<Vec<u8>> = row.get(16)?;
        let stderr_compressed: Option<Vec<u8>> = row.get(17)?;

        let stdout = stdout_text.or_else(|| stdout_compressed.as_deref().and_then(decompress_blob));
        let stderr = stderr_text.or_else(|| stderr_compressed.as_deref().and_then(decompress_blob));

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
            stdout,
            stderr,
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

fn compress_zlib(data: &str) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

/// Insert a command, compressing stdout/stderr with zlib if they exceed max_bytes.
pub fn insert_command_compressed(
    conn: &Connection,
    cmd: &NewCommand,
    max_bytes: usize,
) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();

    let stdout_over = cmd.stdout.is_some_and(|s| s.len() > max_bytes);
    let stderr_over = cmd.stderr.is_some_and(|s| s.len() > max_bytes);

    let stdout_compressed = if stdout_over { cmd.stdout.map(compress_zlib) } else { None };
    let stderr_compressed = if stderr_over { cmd.stderr.map(compress_zlib) } else { None };

    let stdout_text: Option<&str> = if stdout_over { None } else { cmd.stdout };
    let stderr_text: Option<&str> = if stderr_over { None } else { cmd.stderr };

    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, timestamp_end, command_raw, command_binary, command_subcommand, command_args, command_flags, cwd, git_repo, git_branch, exit_code, stdout, stderr, stdout_compressed, stderr_compressed, stdout_truncated, stderr_truncated, env_snapshot, hostname, shell, source, redacted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        rusqlite::params![
            id, cmd.session_id, cmd.timestamp_start, cmd.timestamp_end,
            cmd.command_raw, cmd.command_binary, cmd.command_subcommand,
            cmd.command_args, cmd.command_flags,
            cmd.cwd, cmd.git_repo, cmd.git_branch, cmd.exit_code,
            stdout_text, stderr_text,
            stdout_compressed, stderr_compressed,
            stdout_over || cmd.stdout_truncated, stderr_over || cmd.stderr_truncated,
            cmd.env_snapshot, cmd.hostname, cmd.shell, cmd.source, cmd.redacted,
        ],
    ).map_err(|e| Error::Db(e.to_string()))?;

    // FTS index: use the text column directly when under limit. When compressed,
    // index only a truncated preview — the full content lives in the blob.
    // This prevents FTS from duplicating the data that compression was meant to shrink.
    const FTS_PREVIEW_BYTES: usize = 1024;
    let fts_stdout: Option<String> = if stdout_over {
        cmd.stdout.map(|s| s[..s.len().min(FTS_PREVIEW_BYTES)].to_string())
    } else {
        stdout_text.map(|s| s.to_string())
    };
    let fts_stderr: Option<String> = if stderr_over {
        cmd.stderr.map(|s| s[..s.len().min(FTS_PREVIEW_BYTES)].to_string())
    } else {
        stderr_text.map(|s| s.to_string())
    };

    let rowid: i64 = conn
        .query_row("SELECT rowid FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO commands_fts(rowid, command_raw, stdout, stderr) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![rowid, cmd.command_raw, fts_stdout, fts_stderr],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Update session counters
    conn.execute(
        "UPDATE sessions SET command_count = command_count + 1 WHERE id = ?1",
        [cmd.session_id],
    )
    .ok();

    if cmd.exit_code.is_some_and(|c| c != 0) {
        conn.execute(
            "UPDATE sessions SET error_count = error_count + 1 WHERE id = ?1",
            [cmd.session_id],
        )
        .ok();
    }

    Ok(id)
}

/// Insert a command with automatic secret redaction on command_raw, stdout, and stderr.
pub fn insert_command_redacted(conn: &Connection, cmd: &NewCommand) -> Result<String, Error> {
    insert_command_redacted_with_patterns(conn, cmd, &[])
}

/// Insert with redaction using both built-in and custom patterns.
pub fn insert_command_redacted_with_patterns(
    conn: &Connection,
    cmd: &NewCommand,
    custom: &[crate::core::secrets::engine::CustomPattern],
) -> Result<String, Error> {
    use crate::core::secrets::engine::redact_with_custom_patterns;

    let (redacted_raw, raw_labels) = redact_with_custom_patterns(cmd.command_raw, custom);
    let (redacted_stdout, stdout_labels) = cmd
        .stdout
        .map(|s| redact_with_custom_patterns(s, custom))
        .map(|(s, l)| (Some(s), l))
        .unwrap_or((None, Vec::new()));
    let (redacted_stderr, stderr_labels) = cmd
        .stderr
        .map(|s| redact_with_custom_patterns(s, custom))
        .map(|(s, l)| (Some(s), l))
        .unwrap_or((None, Vec::new()));

    let was_redacted = !raw_labels.is_empty()
        || !stdout_labels.is_empty()
        || !stderr_labels.is_empty();

    let redacted_cmd = NewCommand {
        command_raw: &redacted_raw,
        stdout: redacted_stdout.as_deref(),
        stderr: redacted_stderr.as_deref(),
        redacted: was_redacted,
        ..*cmd
    };

    let cmd_id = insert_command(conn, &redacted_cmd)?;

    // Audit log
    for label in &raw_labels {
        log_redaction(conn, &cmd_id, "command_raw", label)?;
    }
    for label in &stdout_labels {
        log_redaction(conn, &cmd_id, "stdout", label)?;
    }
    for label in &stderr_labels {
        log_redaction(conn, &cmd_id, "stderr", label)?;
    }

    Ok(cmd_id)
}

/// Redact with custom patterns, then compress over-limit stdout/stderr.
pub fn insert_command_redacted_compressed(
    conn: &Connection,
    cmd: &NewCommand,
    custom: &[crate::core::secrets::engine::CustomPattern],
    max_bytes: usize,
) -> Result<String, Error> {
    use crate::core::secrets::engine::redact_with_custom_patterns;

    let (redacted_raw, raw_labels) = redact_with_custom_patterns(cmd.command_raw, custom);
    let (redacted_stdout, stdout_labels) = cmd
        .stdout
        .map(|s| redact_with_custom_patterns(s, custom))
        .map(|(s, l)| (Some(s), l))
        .unwrap_or((None, Vec::new()));
    let (redacted_stderr, stderr_labels) = cmd
        .stderr
        .map(|s| redact_with_custom_patterns(s, custom))
        .map(|(s, l)| (Some(s), l))
        .unwrap_or((None, Vec::new()));

    let was_redacted = !raw_labels.is_empty()
        || !stdout_labels.is_empty()
        || !stderr_labels.is_empty();

    let redacted_cmd = NewCommand {
        command_raw: &redacted_raw,
        stdout: redacted_stdout.as_deref(),
        stderr: redacted_stderr.as_deref(),
        redacted: was_redacted,
        ..*cmd
    };

    let cmd_id = insert_command_compressed(conn, &redacted_cmd, max_bytes)?;

    // Audit log
    for label in &raw_labels {
        log_redaction(conn, &cmd_id, "command_raw", label)?;
    }
    for label in &stdout_labels {
        log_redaction(conn, &cmd_id, "stdout", label)?;
    }
    for label in &stderr_labels {
        log_redaction(conn, &cmd_id, "stderr", label)?;
    }

    Ok(cmd_id)
}

fn log_redaction(conn: &Connection, command_id: &str, field: &str, pattern_label: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO redaction_log (command_id, field, pattern_label) VALUES (?1, ?2, ?3)",
        rusqlite::params![command_id, field, pattern_label],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub struct RedactionLogEntry {
    pub field: String,
    pub pattern_label: String,
    pub redacted_at: i64,
}

pub fn get_redaction_logs(conn: &Connection, command_id: &str) -> Result<Vec<RedactionLogEntry>, Error> {
    let mut stmt = conn
        .prepare("SELECT field, pattern_label, redacted_at FROM redaction_log WHERE command_id = ?1")
        .map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map([command_id], |row| {
            Ok(RedactionLogEntry {
                field: row.get(0)?,
                pattern_label: row.get(1)?,
                redacted_at: row.get(2)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))
}

// --- Agent event insert ---

pub struct AgentEvent {
    pub session_id: String,
    pub command_raw: String,
    pub command_binary: Option<String>,
    pub command_subcommand: Option<String>,
    pub command_args: Option<String>,
    pub command_flags: Option<String>,
    pub cwd: Option<String>,
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub source: String,
    pub agent_session_id: Option<String>,
    pub is_automated: bool,
    pub redacted: bool,
    pub tool_name: String,
    pub tool_input: Option<String>,
    pub tool_response: Option<String>,
}

pub fn insert_agent_event(conn: &Connection, evt: &AgentEvent) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, timestamp_end, command_raw, command_binary, command_subcommand, command_args, command_flags, cwd, git_repo, git_branch, exit_code, stdout, stderr, stdout_truncated, stderr_truncated, source, agent_session_id, is_automated, redacted, tool_name, tool_input, tool_response)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        rusqlite::params![
            id, evt.session_id, now, now,
            evt.command_raw, evt.command_binary, evt.command_subcommand,
            evt.command_args, evt.command_flags,
            evt.cwd, evt.git_repo, evt.git_branch, evt.exit_code,
            evt.stdout, evt.stderr, evt.stdout_truncated, evt.stderr_truncated,
            evt.source, evt.agent_session_id, evt.is_automated, evt.redacted,
            evt.tool_name, evt.tool_input, evt.tool_response,
        ],
    ).map_err(|e| Error::Db(e.to_string()))?;

    // Sync FTS index
    let rowid: i64 = conn
        .query_row("SELECT rowid FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO commands_fts(rowid, command_raw, stdout, stderr) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![rowid, evt.command_raw, evt.stdout, evt.stderr],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Update session counters
    conn.execute(
        "UPDATE sessions SET command_count = command_count + 1, agent_command_count = agent_command_count + 1 WHERE id = ?1",
        [&evt.session_id],
    )
    .ok();

    if evt.exit_code.is_some_and(|c| c != 0) {
        conn.execute(
            "UPDATE sessions SET error_count = error_count + 1 WHERE id = ?1",
            [&evt.session_id],
        )
        .ok();
    }

    Ok(id)
}

/// Find or create a RedTrail session for a given agent session ID.
pub fn find_or_create_agent_session(
    conn: &Connection,
    agent_session_id: &str,
    cwd: Option<&str>,
    source: &str,
) -> Result<String, Error> {
    // Try to find existing session by agent_session_id
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM sessions WHERE agent_session_id = ?1 AND source = ?2",
            rusqlite::params![agent_session_id, source],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new session
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO sessions (id, started_at, cwd_initial, source, agent_session_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, now, cwd, source, agent_session_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(id)
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
        "SELECT c.id, c.session_id, c.command_raw, c.command_binary, c.cwd, c.exit_code, c.hostname, c.shell, c.source, c.timestamp_start, c.timestamp_end, c.stdout, c.stderr, c.stdout_truncated, c.stderr_truncated, c.redacted, c.stdout_compressed, c.stderr_compressed
         FROM commands_fts fts
         JOIN commands c ON c.rowid = fts.rowid
         WHERE commands_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2"
    ).map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
        let stdout_text: Option<String> = row.get(11)?;
        let stderr_text: Option<String> = row.get(12)?;
        let stdout_compressed: Option<Vec<u8>> = row.get(16)?;
        let stderr_compressed: Option<Vec<u8>> = row.get(17)?;

        let stdout = stdout_text.or_else(|| stdout_compressed.as_deref().and_then(decompress_blob));
        let stderr = stderr_text.or_else(|| stderr_compressed.as_deref().and_then(decompress_blob));

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
            stdout,
            stderr,
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
        "DELETE FROM commands_fts WHERE rowid IN (SELECT rowid FROM commands WHERE id = ?1)",
        [id],
    ).map_err(|e| Error::Db(e.to_string()))?;

    conn.execute("DELETE FROM commands WHERE id = ?1", [id])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn forget_session(conn: &Connection, session_id: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM commands_fts WHERE rowid IN (SELECT rowid FROM commands WHERE session_id = ?1)",
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
        "DELETE FROM commands_fts WHERE rowid IN (SELECT rowid FROM commands WHERE timestamp_start >= ?1)",
        [since_ts],
    ).map_err(|e| Error::Db(e.to_string()))?;

    conn.execute("DELETE FROM commands WHERE timestamp_start >= ?1", [since_ts])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Delete commands older than `retention_days`. Also cleans up FTS entries,
/// redaction_log rows, and orphaned sessions.
pub fn enforce_retention(conn: &Connection, retention_days: u32) -> Result<usize, Error> {
    if retention_days == 0 {
        return Ok(0);
    }
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        - (retention_days as i64 * 86_400);

    // Delete FTS entries for expired commands
    conn.execute(
        "DELETE FROM commands_fts WHERE rowid IN (SELECT rowid FROM commands WHERE timestamp_start < ?1)",
        [cutoff],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Delete redaction_log entries for expired commands
    conn.execute(
        "DELETE FROM redaction_log WHERE command_id IN (SELECT id FROM commands WHERE timestamp_start < ?1)",
        [cutoff],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Delete the expired commands
    let deleted = conn
        .execute("DELETE FROM commands WHERE timestamp_start < ?1", [cutoff])
        .map_err(|e| Error::Db(e.to_string()))?;

    // Clean up orphaned sessions (no remaining commands)
    conn.execute(
        "DELETE FROM sessions WHERE id NOT IN (SELECT DISTINCT session_id FROM commands WHERE session_id IS NOT NULL)",
        [],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(deleted)
}
