use crate::error::Error;
use rusqlite::Connection;

use super::decompress_blob;
use super::types::{CommandFilter, CommandRow, NewCommand};

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
        .query_row("SELECT rowid FROM commands WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
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
        "SELECT id, session_id, command_raw, command_binary, cwd, exit_code, hostname, shell, source, timestamp_start, timestamp_end, stdout, stderr, stdout_truncated, stderr_truncated, redacted, stdout_compressed, stderr_compressed, tool_name, command_subcommand, git_repo, git_branch, agent_session_id FROM commands WHERE 1=1",
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
        idx += 1;
    }
    if let Some(agent_sid) = filter.agent_session_id {
        sql.push_str(&format!(" AND agent_session_id = ?{idx}"));
        params.push(Box::new(agent_sid.to_string()));
        idx += 1;
    }
    if let Some(repo) = filter.git_repo {
        sql.push_str(&format!(
            " AND (git_repo = ?{idx} OR cwd LIKE ?{} || '%')",
            idx + 1
        ));
        params.push(Box::new(repo.to_string()));
        params.push(Box::new(repo.to_string()));
        #[allow(unused_assignments)]
        {
            idx += 2;
        }
    }

    sql.push_str(" ORDER BY timestamp_start DESC");

    let limit = filter.limit.unwrap_or(50);
    sql.push_str(&format!(" LIMIT {limit}"));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            let stdout_text: Option<String> = row.get(11)?;
            let stderr_text: Option<String> = row.get(12)?;
            let stdout_compressed: Option<Vec<u8>> = row.get(16)?;
            let stderr_compressed: Option<Vec<u8>> = row.get(17)?;

            let stdout =
                stdout_text.or_else(|| stdout_compressed.as_deref().and_then(decompress_blob));
            let stderr =
                stderr_text.or_else(|| stderr_compressed.as_deref().and_then(decompress_blob));

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
                tool_name: row.get(18)?,
                command_subcommand: row.get(19)?,
                git_repo: row.get(20)?,
                git_branch: row.get(21)?,
                agent_session_id: row.get(22)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
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

    let stdout_compressed = if stdout_over {
        cmd.stdout.map(super::compress_zlib)
    } else {
        None
    };
    let stderr_compressed = if stderr_over {
        cmd.stderr.map(super::compress_zlib)
    } else {
        None
    };

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
        cmd.stdout
            .map(|s| s[..s.len().min(FTS_PREVIEW_BYTES)].to_string())
    } else {
        stdout_text.map(|s| s.to_string())
    };
    let fts_stderr: Option<String> = if stderr_over {
        cmd.stderr
            .map(|s| s[..s.len().min(FTS_PREVIEW_BYTES)].to_string())
    } else {
        stderr_text.map(|s| s.to_string())
    };

    let rowid: i64 = conn
        .query_row("SELECT rowid FROM commands WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
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

    let was_redacted =
        !raw_labels.is_empty() || !stdout_labels.is_empty() || !stderr_labels.is_empty();

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
        super::streaming::log_redaction(conn, &cmd_id, "command_raw", label)?;
    }
    for label in &stdout_labels {
        super::streaming::log_redaction(conn, &cmd_id, "stdout", label)?;
    }
    for label in &stderr_labels {
        super::streaming::log_redaction(conn, &cmd_id, "stderr", label)?;
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

    let was_redacted =
        !raw_labels.is_empty() || !stdout_labels.is_empty() || !stderr_labels.is_empty();

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
        super::streaming::log_redaction(conn, &cmd_id, "command_raw", label)?;
    }
    for label in &stdout_labels {
        super::streaming::log_redaction(conn, &cmd_id, "stdout", label)?;
    }
    for label in &stderr_labels {
        super::streaming::log_redaction(conn, &cmd_id, "stderr", label)?;
    }

    Ok(cmd_id)
}

pub fn search_commands(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<CommandRow>, Error> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.session_id, c.command_raw, c.command_binary, c.cwd, c.exit_code, c.hostname, c.shell, c.source, c.timestamp_start, c.timestamp_end, c.stdout, c.stderr, c.stdout_truncated, c.stderr_truncated, c.redacted, c.stdout_compressed, c.stderr_compressed, c.tool_name, c.command_subcommand, c.git_repo, c.git_branch, c.agent_session_id
         FROM commands_fts fts
         JOIN commands c ON c.rowid = fts.rowid
         WHERE commands_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2"
    ).map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map(rusqlite::params![query, limit as i64], |row| {
            let stdout_text: Option<String> = row.get(11)?;
            let stderr_text: Option<String> = row.get(12)?;
            let stdout_compressed: Option<Vec<u8>> = row.get(16)?;
            let stderr_compressed: Option<Vec<u8>> = row.get(17)?;

            let stdout =
                stdout_text.or_else(|| stdout_compressed.as_deref().and_then(decompress_blob));
            let stderr =
                stderr_text.or_else(|| stderr_compressed.as_deref().and_then(decompress_blob));

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
                tool_name: row.get(18)?,
                command_subcommand: row.get(19)?,
                git_repo: row.get(20)?,
                git_branch: row.get(21)?,
                agent_session_id: row.get(22)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Delete a single command row (and its FTS entry).
pub fn delete_command(conn: &Connection, id: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM commands_fts WHERE rowid IN (SELECT rowid FROM commands WHERE id = ?1)",
        [id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute("DELETE FROM commands WHERE id = ?1", [id])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn forget_command(conn: &Connection, id: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM commands_fts WHERE rowid IN (SELECT rowid FROM commands WHERE id = ?1)",
        [id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

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

    conn.execute(
        "DELETE FROM commands WHERE timestamp_start >= ?1",
        [since_ts],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}
