use rusqlite::Connection;

use crate::core::analysis::{AnalysisResult, analyze_session};
use crate::core::db::{self, CommandFilter, CommandRow};
use crate::core::enrich::run_enrichment;
use crate::core::fmt::ascii::{self, BOLD, CYAN, DIM, GREEN, RED, RESET, YELLOW};
use crate::core::fmt::markdown;
use crate::error::Error;

pub struct AgentReportArgs<'a> {
    pub session: Option<&'a str>,
    pub last: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub json: bool,
    pub markdown: bool,
}

pub fn run(conn: &Connection, args: &AgentReportArgs) -> Result<(), Error> {
    let commands = fetch_commands(conn, args)?;

    if commands.is_empty() {
        print_empty_message(args);
        return Ok(());
    }

    let mut analysis = analyze_session(&commands);

    // Best-effort enrichment based on the source detected in analysis.
    if let Some(source) = &analysis.source {
        let source = source.clone();
        run_enrichment(&source, &mut analysis);
    }

    if args.json {
        print_json(&analysis);
    } else if args.markdown {
        print_markdown(&analysis);
    } else {
        print_ascii(&analysis);
    }

    Ok(())
}

fn fetch_commands(conn: &Connection, args: &AgentReportArgs) -> Result<Vec<CommandRow>, Error> {
    if let Some(session_id) = args.session {
        return db::get_commands(
            conn,
            &CommandFilter {
                agent_session_id: Some(session_id),
                limit: Some(5000),
                ..Default::default()
            },
        );
    }

    if let Some(last) = args.last {
        let since = ascii::parse_duration_ago(last)?;
        let git_repo = resolve_cwd(args.cwd);
        return db::get_commands(
            conn,
            &CommandFilter {
                since: Some(since),
                source: Some("claude_code"),
                git_repo: git_repo.as_deref(),
                limit: Some(5000),
                ..Default::default()
            },
        );
    }

    if let Some(cwd_arg) = args.cwd {
        let git_repo = resolve_cwd(Some(cwd_arg));
        return db::get_commands(
            conn,
            &CommandFilter {
                source: Some("claude_code"),
                git_repo: git_repo.as_deref(),
                limit: Some(5000),
                ..Default::default()
            },
        );
    }

    // Default: find the most recent agent command, then fetch its full session.
    let recent = db::get_commands(
        conn,
        &CommandFilter {
            source: Some("claude_code"),
            limit: Some(1),
            ..Default::default()
        },
    )?;

    if let Some(cmd) = recent.first() {
        if let Some(agent_sid) = &cmd.agent_session_id {
            return db::get_commands(
                conn,
                &CommandFilter {
                    agent_session_id: Some(agent_sid),
                    limit: Some(5000),
                    ..Default::default()
                },
            );
        }
        // Fallback: return just the recent commands from this session_id.
        return db::get_commands(
            conn,
            &CommandFilter {
                session_id: Some(&cmd.session_id),
                source: Some("claude_code"),
                limit: Some(5000),
                ..Default::default()
            },
        );
    }

    Ok(Vec::new())
}

/// Resolve a cwd argument: "." becomes the git repo root, otherwise use as-is.
fn resolve_cwd(cwd: Option<&str>) -> Option<String> {
    let path = cwd?;
    if path == "." {
        // Try git rev-parse --show-toplevel for the current directory.
        std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                } else {
                    // Not a git repo; fall back to canonicalized cwd.
                    std::env::current_dir()
                        .ok()
                        .and_then(|p| p.canonicalize().ok().or(Some(p)))
                        .and_then(|p| p.to_str().map(String::from))
                }
            })
    } else {
        Some(path.to_string())
    }
}

fn print_empty_message(args: &AgentReportArgs) {
    let color = ascii::colors_enabled();
    let (bold, dim, reset_s) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };

    println!("{bold}No agent activity found.{reset_s}");
    println!();

    if args.session.is_some() {
        println!("No commands found for the given session ID.");
    } else if args.last.is_some() {
        println!("No agent commands in the specified time window.");
    } else {
        println!("To start capturing agent activity:");
        println!("  {dim}redtrail setup-hooks{reset_s}");
    }
}

// --- ASCII output ---

fn print_ascii(a: &AnalysisResult) {
    let color = ascii::colors_enabled();
    let (bold, dim, green, red, cyan, yellow, reset_s) = if color {
        (BOLD, DIM, GREEN, RED, CYAN, YELLOW, RESET)
    } else {
        ("", "", "", "", "", "", "")
    };

    let duration = format_duration_human(a.duration_seconds);
    let source_label = a.source.as_deref().unwrap_or("unknown");

    // Header
    println!("{bold}Agent Report{reset_s}");
    println!(
        "  Source: {cyan}{source_label}{reset_s}  Duration: {bold}{duration}{reset_s}  Commands: {bold}{}{reset_s}",
        a.total_commands
    );
    if let Some(dir) = &a.directory {
        print!("  Dir: {dim}{dir}{reset_s}");
    }
    if let Some(branch) = &a.branch {
        print!("  Branch: {cyan}{branch}{reset_s}");
    }
    println!();
    println!();

    // Files
    if !a.files_created.is_empty() || !a.files_modified.is_empty() || !a.files_read_only.is_empty()
    {
        println!("{bold}Files{reset_s}");
        for f in &a.files_created {
            println!("  {green}+ {f}{reset_s}");
        }
        for f in &a.files_modified {
            println!("  {yellow}~ {f}{reset_s}");
        }
        for f in &a.files_read_only {
            println!("  {dim}  {f}{reset_s}");
        }
        println!();
    }

    // Binary stats
    if !a.binary_stats.is_empty() {
        println!("{bold}Commands{reset_s}");
        let mut bins: Vec<_> = a.binary_stats.iter().collect();
        bins.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        for (name, stats) in &bins {
            let fail_str = if stats.failed > 0 {
                format!(" {red}{} failed{reset_s}", stats.failed)
            } else {
                String::new()
            };
            println!(
                "  {name}: {bold}{}{reset_s} total, {green}{} ok{reset_s}{fail_str}",
                stats.total, stats.succeeded
            );
        }
        println!();
    }

    // Errors
    if !a.error_sequences.is_empty() {
        println!("{bold}Error Sequences{reset_s}");
        for seq in &a.error_sequences {
            let status = if seq.resolved {
                format!("{green}resolved{reset_s}")
            } else {
                format!("{red}unresolved{reset_s}")
            };
            println!("  {red}{}{reset_s} [{status}]", seq.failing_command);
            if !seq.fix_actions.is_empty() {
                for action in &seq.fix_actions {
                    println!("    {dim}-> {action}{reset_s}");
                }
            }
            if let Some(res) = &seq.resolution_command {
                println!("    {green}=> {res}{reset_s}");
            }
        }
        println!();
    }

    // Tests
    if a.test_runs > 0 {
        println!("{bold}Tests{reset_s}");
        println!(
            "  {bold}{}{reset_s} runs: {green}{} passed{reset_s}, {red}{} failed{reset_s}",
            a.test_runs, a.tests_passed, a.tests_failed
        );
        println!();
    }

    // Summary
    println!("{bold}Summary{reset_s}");
    println!(
        "  {bold}{}{reset_s} commands ({cyan}{} agent{reset_s}, {dim}{} human{reset_s})",
        a.total_commands, a.agent_commands, a.human_commands
    );
    println!(
        "  {bold}{}{reset_s} errors, {green}{} resolved{reset_s}",
        a.total_errors, a.errors_resolved
    );
}

// --- Markdown output ---

fn print_markdown(a: &AnalysisResult) {
    let duration = format_duration_human(a.duration_seconds);
    let source_label = markdown::escape(a.source.as_deref().unwrap_or("unknown"));

    println!("# Agent Report");
    println!();
    println!("- **Source:** {source_label}");
    println!("- **Duration:** {duration}");
    println!("- **Commands:** {}", a.total_commands);
    if let Some(dir) = &a.directory {
        println!("- **Directory:** `{}`", markdown::escape(dir));
    }
    if let Some(branch) = &a.branch {
        println!("- **Branch:** `{}`", markdown::escape(branch));
    }
    println!();

    if !a.files_created.is_empty() || !a.files_modified.is_empty() || !a.files_read_only.is_empty()
    {
        println!("## Files");
        println!();
        for f in &a.files_created {
            println!("- **created** `{}`", markdown::escape(f));
        }
        for f in &a.files_modified {
            println!("- **modified** `{}`", markdown::escape(f));
        }
        for f in &a.files_read_only {
            println!("- read `{}`", markdown::escape(f));
        }
        println!();
    }

    if !a.binary_stats.is_empty() {
        println!("## Commands");
        println!();
        let mut bins: Vec<_> = a.binary_stats.iter().collect();
        bins.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        for (name, stats) in &bins {
            println!(
                "- `{}`: {} total, {} ok, {} failed",
                markdown::escape(name),
                stats.total,
                stats.succeeded,
                stats.failed
            );
        }
        println!();
    }

    if !a.error_sequences.is_empty() {
        println!("## Error Sequences");
        println!();
        for seq in &a.error_sequences {
            let status = if seq.resolved {
                "resolved"
            } else {
                "unresolved"
            };
            println!(
                "- `{}` [{}]",
                markdown::escape(&seq.failing_command),
                status
            );
            for action in &seq.fix_actions {
                println!("  - {}", markdown::escape(action));
            }
            if let Some(res) = &seq.resolution_command {
                println!("  - **fix:** `{}`", markdown::escape(res));
            }
        }
        println!();
    }

    if a.test_runs > 0 {
        println!("## Tests");
        println!();
        println!(
            "- {} runs: {} passed, {} failed",
            a.test_runs, a.tests_passed, a.tests_failed
        );
        println!();
    }

    println!("## Summary");
    println!();
    println!(
        "- {} commands ({} agent, {} human)",
        a.total_commands, a.agent_commands, a.human_commands
    );
    println!(
        "- {} errors, {} resolved",
        a.total_errors, a.errors_resolved
    );
}

// --- JSON output ---

fn print_json(a: &AnalysisResult) {
    let binary_stats: serde_json::Map<String, serde_json::Value> = a
        .binary_stats
        .iter()
        .map(|(name, stats)| {
            (
                name.clone(),
                serde_json::json!({
                    "total": stats.total,
                    "succeeded": stats.succeeded,
                    "failed": stats.failed,
                }),
            )
        })
        .collect();

    let error_sequences: Vec<serde_json::Value> = a
        .error_sequences
        .iter()
        .map(|seq| {
            serde_json::json!({
                "failing_command": seq.failing_command,
                "error_message": seq.error_message,
                "fix_actions": seq.fix_actions,
                "resolution_command": seq.resolution_command,
                "resolved": seq.resolved,
            })
        })
        .collect();

    let val = serde_json::json!({
        "source": a.source,
        "duration_seconds": a.duration_seconds,
        "directory": a.directory,
        "branch": a.branch,
        "files": {
            "created": a.files_created,
            "modified": a.files_modified,
            "read_only": a.files_read_only,
        },
        "commands": {
            "total": a.total_commands,
            "agent": a.agent_commands,
            "human": a.human_commands,
            "by_binary": binary_stats,
        },
        "errors": error_sequences,
        "tests": {
            "total_runs": a.test_runs,
            "passed": a.tests_passed,
            "failed": a.tests_failed,
        },
    });

    println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
}

// --- Helpers ---

fn format_duration_human(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}
