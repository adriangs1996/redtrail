use crate::error::Error;
use rusqlite::Connection;

use super::types::{AgentEvent, FinishCommand, NewCommandStart, RedactionLogEntry};

/// Insert a minimal row with `status = 'running'`. No stdout/stderr/exit_code yet.
/// Returns the new command ID.
pub fn insert_command_start(conn: &Connection, cmd: &NewCommandStart) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, command_args, command_flags, cwd, shell, hostname, source, redacted, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'running')",
        rusqlite::params![
            id, cmd.session_id, now,
            cmd.command_raw, cmd.command_binary, cmd.command_subcommand,
            cmd.command_args, cmd.command_flags,
            cmd.cwd, cmd.shell, cmd.hostname, cmd.source, cmd.redacted,
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Update session command count (best-effort)
    conn.execute(
        "UPDATE sessions SET command_count = command_count + 1 WHERE id = ?1",
        [cmd.session_id],
    )
    .ok();

    Ok(id)
}

/// Overwrite stdout/stderr on a running command (incremental streaming flush).
/// Only updates rows that are still `status = 'running'` to avoid clobbering finished rows.
pub fn update_command_output(
    conn: &Connection,
    command_id: &str,
    stdout: Option<&str>,
    stderr: Option<&str>,
    stdout_truncated: bool,
    stderr_truncated: bool,
) -> Result<(), Error> {
    conn.execute(
        "UPDATE commands SET stdout = ?2, stderr = ?3, stdout_truncated = ?4, stderr_truncated = ?5
         WHERE id = ?1 AND status = 'running'",
        rusqlite::params![
            command_id,
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Transition a running command to `status = 'finished'`, setting exit metadata and
/// syncing the FTS index with the final content.
pub fn finish_command(conn: &Connection, fc: &FinishCommand) -> Result<(), Error> {
    conn.execute(
        "UPDATE commands
         SET exit_code      = ?2,
             timestamp_end  = unixepoch(),
             git_repo       = ?3,
             git_branch     = ?4,
             env_snapshot   = ?5,
             stdout         = COALESCE(?6, stdout),
             stderr         = COALESCE(?7, stderr),
             status         = 'finished'
         WHERE id = ?1",
        rusqlite::params![
            fc.command_id,
            fc.exit_code,
            fc.git_repo,
            fc.git_branch,
            fc.env_snapshot,
            fc.stdout,
            fc.stderr,
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Sync FTS index — read back the final command_raw/stdout/stderr
    let (command_raw, stdout, stderr, rowid): (String, Option<String>, Option<String>, i64) = conn
        .query_row(
            "SELECT command_raw, stdout, stderr, rowid FROM commands WHERE id = ?1",
            [fc.command_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    conn.execute(
        "INSERT INTO commands_fts(rowid, command_raw, stdout, stderr) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![rowid, command_raw, stdout, stderr],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Update session error count if exit_code != 0
    if fc.exit_code.is_some_and(|c| c != 0) {
        // Retrieve session_id first
        let session_id: String = conn
            .query_row(
                "SELECT session_id FROM commands WHERE id = ?1",
                [fc.command_id],
                |r| r.get(0),
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET error_count = error_count + 1 WHERE id = ?1",
            [&session_id],
        )
        .ok();
    }

    Ok(())
}

/// Read stdout/stderr for `command_id`. If either exceeds `max_bytes`, compress with
/// zlib and update the row (stdout=NULL, stdout_compressed=blob, stdout_truncated=1).
pub fn compress_command_output_if_needed(
    conn: &Connection,
    command_id: &str,
    max_bytes: usize,
) -> Result<(), Error> {
    let (stdout, stderr): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT stdout, stderr FROM commands WHERE id = ?1",
            [command_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let stdout_over = stdout.as_deref().is_some_and(|s| s.len() > max_bytes);
    let stderr_over = stderr.as_deref().is_some_and(|s| s.len() > max_bytes);

    if !stdout_over && !stderr_over {
        return Ok(());
    }

    let stdout_compressed: Option<Vec<u8>> = if stdout_over {
        stdout.as_deref().map(super::compress_zlib)
    } else {
        None
    };
    let stderr_compressed: Option<Vec<u8>> = if stderr_over {
        stderr.as_deref().map(super::compress_zlib)
    } else {
        None
    };

    conn.execute(
        "UPDATE commands
         SET stdout            = CASE WHEN ?2 THEN NULL ELSE stdout END,
             stdout_compressed = CASE WHEN ?2 THEN ?4 ELSE stdout_compressed END,
             stdout_truncated  = CASE WHEN ?2 THEN 1 ELSE stdout_truncated END,
             stderr            = CASE WHEN ?3 THEN NULL ELSE stderr END,
             stderr_compressed = CASE WHEN ?3 THEN ?5 ELSE stderr_compressed END,
             stderr_truncated  = CASE WHEN ?3 THEN 1 ELSE stderr_truncated END
         WHERE id = ?1",
        rusqlite::params![
            command_id,
            stdout_over,
            stderr_over,
            stdout_compressed,
            stderr_compressed,
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(())
}

/// Mark as `orphaned` any `running` commands whose `timestamp_start` is more than
/// 24 hours in the past. Not scoped to a single session — stale commands from any
/// session are cleaned up. Returns the number of rows updated.
pub fn cleanup_orphaned_commands(conn: &Connection) -> Result<usize, Error> {
    let affected = conn
        .execute(
            "UPDATE commands SET status = 'orphaned'
             WHERE status = 'running'
               AND timestamp_start < unixepoch() - 86400",
            [],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(affected)
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

pub fn log_redaction(
    conn: &Connection,
    command_id: &str,
    field: &str,
    pattern_label: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO redaction_log (command_id, field, pattern_label) VALUES (?1, ?2, ?3)",
        rusqlite::params![command_id, field, pattern_label],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn get_redaction_logs(
    conn: &Connection,
    command_id: &str,
) -> Result<Vec<RedactionLogEntry>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT field, pattern_label, redacted_at FROM redaction_log WHERE command_id = ?1",
        )
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
        .query_row("SELECT rowid FROM commands WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
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
