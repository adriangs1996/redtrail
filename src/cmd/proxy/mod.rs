mod pty;

use redtrail::context::AppContext;
use redtrail::core;
use redtrail::error::Error;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub struct ProxyArgs {
    pub command: Vec<String>,
}

pub fn run(ctx: &AppContext, args: &ProxyArgs) -> Result<(), Error> {
    if args.command.is_empty() {
        return Ok(());
    }

    let cmd_str = args.command.join(" ");
    let tool = core::extractor::detect_tool(&cmd_str, None);

    let result = pty::spawn_and_capture(&args.command)?;

    let hash = format!("{:x}", Sha256::digest(result.output.as_bytes()));

    let event_id = core::db::insert_event(
        &ctx.conn,
        &ctx.session_id,
        &cmd_str,
        tool.as_deref(),
        result.exit_code,
        result.duration_ms,
        &result.output,
        &hash,
    )?;

    let extraction = core::extractor::synthetize(&cmd_str, tool.as_deref(), &result.output);

    if !extraction.is_empty() {
        core::db::store_extraction(
            &ctx.conn, &ctx.session_id, event_id,
            &extraction.facts, &extraction.relations,
        )?;

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for fact in &extraction.facts {
            *counts.entry(fact.fact_type.as_str()).or_insert(0) += 1;
        }
        let mut parts: Vec<String> = counts.iter()
            .map(|(ft, n)| format!("{n} {ft}"))
            .collect();
        parts.sort();
        if !extraction.relations.is_empty() {
            parts.push(format!("{} relations", extraction.relations.len()));
        }
        eprintln!("[rt] extracted {}", parts.join(", "));
    }

    if result.exit_code != 0 {
        std::process::exit(result.exit_code);
    }
    Ok(())
}
