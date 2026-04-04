use crate::error::Error;
use rusqlite::Connection;

pub(super) const SCHEMA: &str = "
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
    extraction_method TEXT,
    created_at INTEGER DEFAULT (unixepoch()),
    status TEXT NOT NULL DEFAULT 'finished',
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
    canonical_key TEXT NOT NULL DEFAULT '',
    properties TEXT,
    first_seen INTEGER,
    last_seen INTEGER,
    source_command_id TEXT,
    FOREIGN KEY (source_command_id) REFERENCES commands(id),
    UNIQUE(type, canonical_key)
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
CREATE INDEX IF NOT EXISTS idx_relationships_type ON relationships(type);

-- Entity observations (Phase 2)
CREATE TABLE IF NOT EXISTS entity_observations (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id),
    command_id TEXT NOT NULL REFERENCES commands(id),
    observed_at INTEGER NOT NULL,
    context TEXT,
    UNIQUE(entity_id, command_id)
);
CREATE INDEX IF NOT EXISTS idx_entity_obs_entity ON entity_observations(entity_id, observed_at);
CREATE INDEX IF NOT EXISTS idx_entity_obs_command ON entity_observations(command_id);

-- Git typed tables (Phase 2)
CREATE TABLE IF NOT EXISTS git_branches (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    is_remote BOOLEAN DEFAULT 0,
    remote_name TEXT,
    upstream TEXT,
    ahead INTEGER,
    behind INTEGER,
    last_commit_hash TEXT,
    UNIQUE(repo, name, is_remote)
);

CREATE TABLE IF NOT EXISTS git_commits (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    hash TEXT NOT NULL,
    short_hash TEXT,
    author_name TEXT,
    author_email TEXT,
    message TEXT,
    committed_at INTEGER,
    UNIQUE(repo, hash)
);

CREATE TABLE IF NOT EXISTS git_remotes (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    url TEXT,
    UNIQUE(repo, name)
);

CREATE TABLE IF NOT EXISTS git_files (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    path TEXT NOT NULL,
    status TEXT,
    insertions INTEGER,
    deletions INTEGER,
    UNIQUE(repo, path)
);

CREATE TABLE IF NOT EXISTS git_tags (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    commit_hash TEXT,
    UNIQUE(repo, name)
);

CREATE TABLE IF NOT EXISTS git_stashes (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    index_num INTEGER,
    message TEXT NOT NULL,
    UNIQUE(repo, message)
);

-- Docker typed tables (Phase 2)
CREATE TABLE IF NOT EXISTS docker_containers (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    container_id TEXT,
    name TEXT NOT NULL,
    image TEXT,
    status TEXT,
    ports TEXT,
    created_at_container INTEGER,
    UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS docker_images (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repository TEXT NOT NULL,
    tag TEXT,
    image_id TEXT,
    size_bytes INTEGER,
    created_at_image INTEGER,
    UNIQUE(repository, tag)
);

CREATE TABLE IF NOT EXISTS docker_networks (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    name TEXT NOT NULL,
    network_id TEXT,
    driver TEXT,
    UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS docker_volumes (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    name TEXT NOT NULL,
    driver TEXT,
    mountpoint TEXT,
    UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS docker_services (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    name TEXT NOT NULL,
    image TEXT,
    compose_file TEXT,
    ports TEXT,
    UNIQUE(name, compose_file)
);

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

pub(super) fn init(conn: &Connection) -> Result<(), Error> {
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
    migrate_status_column(conn)?;
    migrate_extraction_tables(conn)?;
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
             ALTER TABLE commands ADD COLUMN stderr_compressed BLOB;",
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    }
    Ok(())
}

/// Add status column (for existing databases that predate streaming capture).
fn migrate_status_column(conn: &Connection) -> Result<(), Error> {
    let has_col: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .map_err(|e| Error::Db(e.to_string()))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .any(|col| col.as_deref() == Ok("status"));

    if !has_col {
        conn.execute_batch(
            "ALTER TABLE commands ADD COLUMN status TEXT NOT NULL DEFAULT 'finished';",
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_commands_status ON commands(status) WHERE status = 'running';"
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Add extraction_method on commands and canonical_key on entities (for existing databases).
fn migrate_extraction_tables(conn: &Connection) -> Result<(), Error> {
    let has_extraction_method: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .map_err(|e| Error::Db(e.to_string()))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .any(|col| col.as_deref() == Ok("extraction_method"));

    if !has_extraction_method {
        conn.execute_batch("ALTER TABLE commands ADD COLUMN extraction_method TEXT;")
            .map_err(|e| Error::Db(e.to_string()))?;
    }

    let has_canonical_key: bool = conn
        .prepare("PRAGMA table_info(entities)")
        .map_err(|e| Error::Db(e.to_string()))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .any(|col| col.as_deref() == Ok("canonical_key"));

    if !has_canonical_key {
        conn.execute_batch(
            "ALTER TABLE entities ADD COLUMN canonical_key TEXT NOT NULL DEFAULT '';
             UPDATE entities SET canonical_key = name WHERE canonical_key = '';
             CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_type_canonical ON entities(type, canonical_key);",
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    }

    Ok(())
}

#[cfg(unix)]
pub(super) fn set_file_permissions(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
pub(super) fn set_file_permissions(_path: &str) {}
