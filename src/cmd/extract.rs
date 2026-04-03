use crate::core::fmt::ascii;
use crate::error::Error;
use crate::extract;
use crate::extract::db::{get_entities, get_unextracted_commands, EntityFilter};
use rusqlite::Connection;

pub struct ExtractArgs {
    pub reprocess: bool,
    pub since: Option<i64>,
    pub dry_run: bool,
    pub limit: Option<usize>,
}

pub fn run(conn: &Connection, args: &ExtractArgs) -> Result<(), Error> {
    let limit = args.limit.unwrap_or(1000);

    // Get commands to process
    let commands = if args.reprocess {
        // Re-extract all finished commands regardless of extracted flag
        let mut sql = String::from(
            "SELECT id, session_id, command_raw, command_binary, cwd, exit_code, hostname, shell, source, timestamp_start, timestamp_end, stdout, stderr, stdout_truncated, stderr_truncated, redacted, stdout_compressed, stderr_compressed, tool_name, command_subcommand, git_repo, git_branch, agent_session_id
             FROM commands WHERE status = 'finished'",
        );
        if let Some(ts) = args.since {
            sql.push_str(&format!(" AND timestamp_start >= {ts}"));
        }
        sql.push_str(&format!(" ORDER BY timestamp_start ASC LIMIT {limit}"));
        query_commands_raw(conn, &sql)?
    } else {
        get_unextracted_commands(conn, args.since, limit)?
    };

    let total = commands.len();
    if total == 0 {
        println!(
            "{}No commands to extract.{}",
            ascii::DIM,
            ascii::RESET
        );
        return Ok(());
    }

    if args.dry_run {
        println!(
            "{BOLD}Dry run:{RESET} would process {total} commands",
            BOLD = ascii::BOLD,
            RESET = ascii::RESET,
        );
        for cmd in &commands {
            println!(
                "  {DIM}[{id}]{RESET} {raw}",
                DIM = ascii::DIM,
                RESET = ascii::RESET,
                id = &cmd.id[..8.min(cmd.id.len())],
                raw = ascii::truncate_command(&cmd.command_raw, 80),
            );
        }
        return Ok(());
    }

    // Count entities before extraction for delta
    let entities_before = get_entities(conn, &EntityFilter::default())
        .map(|v| v.len())
        .unwrap_or(0);

    let mut processed = 0usize;
    let mut errors = 0usize;

    for cmd in &commands {
        // If reprocessing, reset the extracted flag so extract_command() doesn't skip
        if args.reprocess {
            let _ = conn.execute(
                "UPDATE commands SET extracted = 0, extraction_method = NULL WHERE id = ?1",
                [&cmd.id],
            );
        }
        match extract::extract_command(conn, cmd) {
            Ok(()) => processed += 1,
            Err(e) => {
                eprintln!(
                    "[redtrail] extraction error (cmd={}): {e}",
                    &cmd.id[..8.min(cmd.id.len())]
                );
                errors += 1;
            }
        }
        // Print progress every 50 commands
        if processed.is_multiple_of(50) && processed > 0 {
            eprintln!("  ... {processed}/{total}");
        }
    }

    let entities_after = get_entities(conn, &EntityFilter::default())
        .map(|v| v.len())
        .unwrap_or(0);
    let new_entities = entities_after.saturating_sub(entities_before);

    if errors > 0 {
        println!(
            "{GREEN}Extracted {processed}/{total} commands{RESET} {DIM}({new_entities} entities created, {RED}{errors} errors{RESET}{DIM}){RESET}",
            GREEN = ascii::GREEN,
            RED = ascii::RED,
            DIM = ascii::DIM,
            RESET = ascii::RESET,
        );
    } else {
        println!(
            "{GREEN}Extracted {processed}/{total} commands{RESET} {DIM}({new_entities} entities created){RESET}",
            GREEN = ascii::GREEN,
            DIM = ascii::DIM,
            RESET = ascii::RESET,
        );
    }

    Ok(())
}

/// Execute a raw SQL query returning CommandRows. Used for reprocess mode.
fn query_commands_raw(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<crate::core::db::CommandRow>, Error> {
    use crate::core::db::CommandRow;

    let mut stmt = conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            let stdout_text: Option<String> = r.get(11)?;
            let stderr_text: Option<String> = r.get(12)?;
            let stdout_compressed: Option<Vec<u8>> = r.get(16)?;
            let stderr_compressed: Option<Vec<u8>> = r.get(17)?;

            let stdout = stdout_text.or_else(|| {
                stdout_compressed
                    .as_deref()
                    .and_then(decompress_blob)
            });
            let stderr = stderr_text.or_else(|| {
                stderr_compressed
                    .as_deref()
                    .and_then(decompress_blob)
            });

            Ok(CommandRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                command_raw: r.get(2)?,
                command_binary: r.get(3)?,
                cwd: r.get(4)?,
                exit_code: r.get(5)?,
                hostname: r.get(6)?,
                shell: r.get(7)?,
                source: r.get(8)?,
                timestamp_start: r.get(9)?,
                timestamp_end: r.get(10)?,
                stdout,
                stderr,
                stdout_truncated: r.get(13)?,
                stderr_truncated: r.get(14)?,
                redacted: r.get(15)?,
                tool_name: r.get(18)?,
                command_subcommand: r.get(19)?,
                git_repo: r.get(20)?,
                git_branch: r.get(21)?,
                agent_session_id: r.get(22)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

fn decompress_blob(blob: &[u8]) -> Option<String> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(blob);
    let mut out = String::new();
    decoder.read_to_string(&mut out).ok()?;
    Some(out)
}
