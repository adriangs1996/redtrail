use std::collections::HashMap;
use std::io::Read as _;

use rusqlite::Connection;

use crate::core::db::{self, CommandFilter, CommandRow};
use crate::core::errors::{extract_error_lines, normalize_error};
use crate::core::fmt::ascii::{
    BOLD, DIM, GREEN, RED, RESET, YELLOW, colors_enabled, format_relative_time,
};
use crate::error::Error;

pub struct ResolveArgs<'a> {
    pub error: Option<&'a str>,
    pub stdin: bool,
    pub cwd: Option<&'a str>,
    pub cmd: Option<&'a str>,
    pub global: bool,
    pub json: bool,
}

/// A single resolution: the command that fixed an error.
#[derive(Debug, Clone)]
struct Resolution {
    fix_command: String,
}

/// An aggregated error pattern with its resolutions.
#[derive(Debug, Clone)]
struct ErrorPattern {
    normalized: String,
    raw_sample: String,
    occurrences: usize,
    last_seen: i64,
    resolutions: Vec<Resolution>,
}

/// Serializable result for JSON output.
#[derive(serde::Serialize)]
struct JsonResult {
    error_pattern: String,
    raw_sample: String,
    occurrences: usize,
    last_seen: i64,
    resolutions: Vec<JsonResolution>,
    success_rate: f64,
}

#[derive(serde::Serialize)]
struct JsonResolution {
    command: String,
    count: usize,
}

pub fn run(conn: &Connection, args: &ResolveArgs) -> Result<(), Error> {
    let error_input = get_error_input(args)?;

    if error_input.len() < 5 {
        return Err(Error::Db(
            "Error input too short (< 5 characters). Provide a more specific error message."
                .to_string(),
        ));
    }

    let normalized = normalize_error(&error_input);
    let error_lines = extract_error_lines(&error_input);

    // Resolve scoping
    let git_repo = if args.global {
        None
    } else {
        resolve_scope(args.cwd)
    };

    // Strategy A: FTS5 search
    let mut matches = search_fts(conn, &error_lines, git_repo.as_deref(), args.cmd)?;

    // Strategy B: fallback to failed commands with stderr matching
    if matches.is_empty() {
        matches = search_failed_commands(conn, &normalized, git_repo.as_deref(), args.cmd)?;
    }

    // Auto-widen to global if no local matches (Decision #16)
    let widened = if matches.is_empty() && git_repo.is_some() && !args.global {
        matches = search_fts(conn, &error_lines, None, args.cmd)?;
        if matches.is_empty() {
            matches = search_failed_commands(conn, &normalized, None, args.cmd)?;
        }
        !matches.is_empty()
    } else {
        false
    };

    if matches.is_empty() {
        print_no_matches(args.global);
        return Ok(());
    }

    // Find resolutions for each match
    let patterns = build_patterns(conn, &matches, &normalized)?;

    if patterns.is_empty() {
        print_no_matches(args.global);
        return Ok(());
    }

    // Rank: success_rate first, frequency tiebreaker (Decision #15)
    let ranked = rank_patterns(patterns);

    if args.json {
        print_json(&ranked);
    } else {
        print_ascii(&ranked, widened);
    }

    Ok(())
}

fn get_error_input(args: &ResolveArgs) -> Result<String, Error> {
    if args.stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(Error::Io)?;
        let extracted = extract_error_lines(&buf);
        Ok(extracted)
    } else if let Some(err) = args.error {
        Ok(err.to_string())
    } else {
        Err(Error::Db(
            "No error provided. Pass an error message as argument or use --stdin.".to_string(),
        ))
    }
}

fn resolve_scope(cwd_arg: Option<&str>) -> Option<String> {
    let dir = if let Some(c) = cwd_arg {
        if c == "." {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.canonicalize().ok().or(Some(p)))
                .and_then(|p| p.to_str().map(String::from))
        } else {
            Some(c.to_string())
        }
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.canonicalize().ok().or(Some(p)))
            .and_then(|p| p.to_str().map(String::from))
    };

    dir.and_then(|d| {
        let ctx = crate::core::capture::git_context(&d);
        ctx.repo
    })
}

fn search_fts(
    conn: &Connection,
    query: &str,
    git_repo: Option<&str>,
    cmd_filter: Option<&str>,
) -> Result<Vec<CommandRow>, Error> {
    // Build FTS query from the error lines - take key terms
    let fts_query = build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = db::search_commands(conn, &fts_query, 200)?;

    // Filter to failed commands only
    results.retain(|c| c.exit_code.is_some_and(|code| code != 0));

    // Apply scope filters
    if let Some(repo) = git_repo {
        results.retain(|c| {
            c.git_repo.as_deref() == Some(repo)
                || c.cwd.as_deref().is_some_and(|cwd| cwd.starts_with(repo))
        });
    }
    if let Some(bin) = cmd_filter {
        results.retain(|c| c.command_binary.as_deref() == Some(bin));
    }

    Ok(results)
}

fn build_fts_query(input: &str) -> String {
    // Extract meaningful words for FTS, skip very short or common words
    let stop_words = [
        "the", "a", "an", "is", "at", "in", "on", "for", "to", "of", "and", "or", "not", "no",
        "with", "from", "by", "as", "was", "but", "are", "has", "had", "have",
    ];

    let words: Vec<&str> = input
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .filter(|w| !stop_words.contains(&w.to_lowercase().as_str()))
        .take(8)
        .collect();

    if words.is_empty() {
        return String::new();
    }

    // Use OR so partial matches still hit
    words.join(" OR ")
}

fn search_failed_commands(
    conn: &Connection,
    normalized_error: &str,
    git_repo: Option<&str>,
    cmd_filter: Option<&str>,
) -> Result<Vec<CommandRow>, Error> {
    let mut results = db::get_commands(
        conn,
        &CommandFilter {
            failed_only: true,
            command_binary: cmd_filter,
            git_repo,
            limit: Some(500),
            ..Default::default()
        },
    )?;

    // Filter by stderr similarity to the normalized error
    results.retain(|c| {
        if let Some(stderr) = &c.stderr {
            let norm = normalize_error(stderr);
            normalized_error_matches(&norm, normalized_error)
        } else {
            false
        }
    });

    Ok(results)
}

/// Check if two normalized errors are similar enough to be considered the same pattern.
fn normalized_error_matches(candidate: &str, query: &str) -> bool {
    // If query is a substring of candidate (or vice versa), it's a match
    if candidate.contains(query) || query.contains(candidate) {
        return true;
    }

    // Check word overlap: if >= 60% of query words appear in candidate
    let query_words: Vec<&str> = query.split_whitespace().collect();
    if query_words.is_empty() {
        return false;
    }
    let matching = query_words
        .iter()
        .filter(|w| candidate.contains(**w))
        .count();
    let ratio = matching as f64 / query_words.len() as f64;
    ratio >= 0.6
}

fn build_patterns(
    conn: &Connection,
    failed_commands: &[CommandRow],
    normalized_query: &str,
) -> Result<Vec<ErrorPattern>, Error> {
    let mut pattern_map: HashMap<String, ErrorPattern> = HashMap::new();

    for cmd in failed_commands {
        let stderr = cmd.stderr.as_deref().unwrap_or("");
        let norm = normalize_error(stderr);

        // Use the normalized error as the grouping key, but try to match
        // against the query to avoid grouping unrelated errors
        let key = if normalized_error_matches(&norm, normalized_query) {
            // Use the normalized form of this specific error for grouping
            norm.clone()
        } else {
            continue;
        };

        let entry = pattern_map.entry(key).or_insert_with(|| ErrorPattern {
            normalized: norm,
            raw_sample: extract_error_lines(stderr),
            occurrences: 0,
            last_seen: cmd.timestamp_start,
            resolutions: Vec::new(),
        });

        entry.occurrences += 1;
        if cmd.timestamp_start > entry.last_seen {
            entry.last_seen = cmd.timestamp_start;
        }

        // Find resolution: next successful command of same binary in same session within 10 min
        if let Some(resolution) = find_resolution(conn, cmd)? {
            entry.resolutions.push(resolution);
        }
    }

    Ok(pattern_map.into_values().collect())
}

fn find_resolution(
    conn: &Connection,
    failed_cmd: &CommandRow,
) -> Result<Option<Resolution>, Error> {
    let binary = match &failed_cmd.command_binary {
        Some(b) => b.as_str(),
        None => return Ok(None),
    };

    let max_ts = failed_cmd.timestamp_start + 600; // 10 minutes

    // Look for next successful command of same binary in same session
    let mut stmt = conn
        .prepare(
            "SELECT command_raw
             FROM commands
             WHERE session_id = ?1
               AND command_binary = ?2
               AND exit_code = 0
               AND timestamp_start > ?3
               AND timestamp_start <= ?4
             ORDER BY timestamp_start ASC
             LIMIT 1",
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let result = stmt
        .query_row(
            rusqlite::params![
                failed_cmd.session_id,
                binary,
                failed_cmd.timestamp_start,
                max_ts
            ],
            |row| {
                Ok(Resolution {
                    fix_command: row.get(0)?,
                })
            },
        )
        .ok();

    Ok(result)
}

fn rank_patterns(mut patterns: Vec<ErrorPattern>) -> Vec<ErrorPattern> {
    patterns.sort_by(|a, b| {
        let rate_a = success_rate(a);
        let rate_b = success_rate(b);

        // Higher success rate first
        rate_b
            .partial_cmp(&rate_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                // Frequency tiebreaker: more occurrences first
                b.occurrences.cmp(&a.occurrences)
            })
    });

    patterns
}

fn success_rate(pattern: &ErrorPattern) -> f64 {
    if pattern.occurrences == 0 {
        return 0.0;
    }
    pattern.resolutions.len() as f64 / pattern.occurrences as f64
}

fn print_no_matches(is_global: bool) {
    let color = colors_enabled();
    if color {
        eprintln!("{DIM}No matching errors found in RedTrail history.{RESET}");
    } else {
        eprintln!("No matching errors found in RedTrail history.");
    }
    if !is_global {
        eprintln!("Tip: try --global to search across all projects.");
    }
}

fn print_ascii(patterns: &[ErrorPattern], widened: bool) {
    let color = colors_enabled();

    if widened {
        if color {
            eprintln!("{YELLOW}No matches in current project; showing global results.{RESET}");
        } else {
            eprintln!("No matches in current project; showing global results.");
        }
    }

    println!("Found {} matching error pattern(s).", patterns.len());
    println!();

    for pattern in patterns {
        let sample = truncate_error(&pattern.raw_sample, 120);
        let last = format_relative_time(pattern.last_seen);

        if color {
            println!("  {RED}Error:{RESET} \"{sample}\"");
            println!(
                "  {DIM}Seen:{RESET} {} time(s) (last: {})",
                pattern.occurrences, last
            );
        } else {
            println!("  Error: \"{sample}\"");
            println!("  Seen: {} time(s) (last: {})", pattern.occurrences, last);
        }

        // Deduplicate and count resolutions
        let fix_counts = deduplicate_resolutions(&pattern.resolutions);

        if fix_counts.is_empty() {
            if color {
                println!("  {DIM}No known fix found.{RESET}");
            } else {
                println!("  No known fix found.");
            }
        } else {
            for (i, (fix_cmd, count)) in fix_counts.iter().enumerate() {
                if color {
                    println!(
                        "  {GREEN}Fix #{}: {BOLD}{}{RESET} (worked {}/{})",
                        i + 1,
                        fix_cmd,
                        count,
                        pattern.occurrences
                    );
                } else {
                    println!(
                        "  Fix #{}: {} (worked {}/{})",
                        i + 1,
                        fix_cmd,
                        count,
                        pattern.occurrences
                    );
                }
            }
        }

        println!();
    }
}

fn print_json(patterns: &[ErrorPattern]) {
    let results: Vec<JsonResult> = patterns
        .iter()
        .map(|p| {
            let fix_counts = deduplicate_resolutions(&p.resolutions);
            JsonResult {
                error_pattern: p.normalized.clone(),
                raw_sample: p.raw_sample.clone(),
                occurrences: p.occurrences,
                last_seen: p.last_seen,
                resolutions: fix_counts
                    .into_iter()
                    .map(|(cmd, count)| JsonResolution {
                        command: cmd,
                        count,
                    })
                    .collect(),
                success_rate: success_rate(p),
            }
        })
        .collect();

    if let Ok(json) = serde_json::to_string_pretty(&results) {
        println!("{json}");
    }
}

/// Deduplicate resolutions by command string, returning (command, count) sorted by count desc.
fn deduplicate_resolutions(resolutions: &[Resolution]) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in resolutions {
        *counts.entry(r.fix_command.clone()).or_insert(0) += 1;
    }
    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted
}

fn truncate_error(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max.saturating_sub(3)])
    }
}
