pub mod db;
pub mod docker;
pub mod domain;
pub mod generic;
pub mod git;
pub mod llm;
pub mod parse;
pub mod types;

use crate::config::LlmConfig;
use crate::core::db::CommandRow;
use crate::extract::types::{Domain, DomainExtractor, ExtractError, Extraction};
use rusqlite::Connection;

/// Extract entities and relationships from a captured command.
/// Stores results in the DB and marks the command as extracted.
///
/// This is the central pipeline coordinator:
///   detect domain → dispatch extractor → run generic → merge → store → mark
///
/// Errors from domain extractors are non-fatal and logged to stderr; the pipeline
/// continues and falls back to generic extraction. This ensures a single bad
/// command never breaks the extraction loop.
pub fn extract_command(
    conn: &Connection,
    cmd: &CommandRow,
    llm_config: Option<&LlmConfig>,
) -> Result<(), ExtractError> {
    // 1. Skip already-extracted commands — idempotent by design.
    let already_extracted: bool = conn
        .query_row(
            "SELECT extracted FROM commands WHERE id = ?1",
            [&cmd.id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if already_extracted {
        return Ok(());
    }

    // 2. Resolve stdout. CommandRow.stdout may already be populated (inline text
    //    or already-decompressed by the db layer), but fall back to a fresh DB
    //    read to handle the compressed-blob path.
    let stdout = cmd.stdout.clone().or_else(|| {
        db::get_command_output(conn, &cmd.id)
            .ok()
            .and_then(|(s, _)| s)
    });

    // 3. Nothing to extract → mark skipped and bail.
    let stdout_text = match stdout {
        Some(ref s) if !s.trim().is_empty() => s.as_str(),
        _ => {
            db::mark_extracted(conn, &cmd.id, "skipped")
                .map_err(|e| ExtractError::Db(e.to_string()))?;
            return Ok(());
        }
    };

    // 4. Detect domain from the binary name.
    let binary = cmd.command_binary.as_deref().unwrap_or("");
    let domain = domain::detect_domain(binary);

    // 5. Run domain-specific extractor (errors are non-fatal; log and continue).
    let domain_extraction = match domain {
        Domain::Git => {
            let extractor = git::GitExtractor;
            extractor.extract(cmd).unwrap_or_else(|e| {
                eprintln!("[redtrail] git extractor error (cmd={}): {e}", cmd.id);
                Extraction::empty()
            })
        }
        Domain::Docker => {
            let extractor = docker::DockerExtractor;
            extractor.extract(cmd).unwrap_or_else(|e| {
                eprintln!("[redtrail] docker extractor error (cmd={}): {e}", cmd.id);
                Extraction::empty()
            })
        }
        Domain::Generic => Extraction::empty(),
    };
    let domain_had_results = !domain_extraction.is_empty();

    // 6. Run generic extractor on the same stdout (always, as a supplement).
    //    We need a CommandRow with stdout populated for the generic extractor.
    let cmd_with_stdout = if cmd.stdout.is_some() {
        std::borrow::Cow::Borrowed(cmd)
    } else {
        let mut c = cmd.clone();
        c.stdout = Some(stdout_text.to_string());
        std::borrow::Cow::Owned(c)
    };
    let generic_extraction = {
        let extractor = generic::GenericExtractor;
        extractor.extract(&cmd_with_stdout).unwrap_or_else(|e| {
            eprintln!("[redtrail] generic extractor error (cmd={}): {e}", cmd.id);
            Extraction::empty()
        })
    };

    // 7. Merge domain + generic results.
    let mut combined = domain_extraction;
    combined.merge(generic_extraction);

    // 7b. LLM fallback — only when heuristics produced nothing.
    let mut llm_produced = false;
    if combined.is_empty()
        && let Some(cfg) = llm_config
        && let Some(extractor) = llm::LlmExtractor::new(cfg)
    {
        let llm_result = extractor.extract(&cmd_with_stdout, stdout_text);
        if !llm_result.is_empty() {
            llm_produced = true;
            combined.merge(llm_result);
        }
    }

    // 8-10. Atomically store all entities and relationships.
    if !combined.is_empty() {
        conn.execute_batch("BEGIN")
            .map_err(|e| ExtractError::Db(e.to_string()))?;

        for entity in &combined.entities {
            if let Err(e) = db::upsert_entity(conn, entity, &cmd.id, cmd.timestamp_start) {
                eprintln!(
                    "[redtrail] entity upsert failed (cmd={}, key={}): {e}",
                    cmd.id, entity.canonical_key
                );
            }
        }
        for rel in &combined.relationships {
            if let Err(e) =
                db::insert_relationship(conn, rel, &cmd.id, cmd.timestamp_start)
            {
                // Non-fatal: the referenced entity may have been filtered or not upserted
                // (e.g., unsupported typed table for this entity type).
                eprintln!(
                    "[redtrail] relationship insert failed (cmd={}, {}->{}: {}): {e}",
                    cmd.id,
                    rel.source_canonical_key,
                    rel.target_canonical_key,
                    rel.relation_type
                );
            }
        }

        conn.execute_batch("COMMIT")
            .map_err(|e| ExtractError::Db(e.to_string()))?;
    }

    // 11-12. Choose extraction method and mark the command as done.
    let method = if domain_had_results {
        "heuristic"
    } else if llm_produced {
        "llm"
    } else if !combined.is_empty() {
        "generic"
    } else {
        "skipped"
    };
    db::mark_extracted(conn, &cmd.id, method)
        .map_err(|e| ExtractError::Db(e.to_string()))?;

    Ok(())
}
