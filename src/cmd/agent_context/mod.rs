// Agent context command: generates a context document for new AI agent sessions.
// Two modes: fast (heuristic-only) and smart (LLM-powered).

mod fast;
mod filter;
mod git_state;
mod llm;
mod smart;

use std::collections::HashMap;

use rusqlite::Connection;

use crate::config::Config;
use crate::core::analysis::analyze_session;
use crate::core::db::{self, CommandFilter, CommandRow};
use crate::core::fmt::ascii::parse_duration_ago;
use crate::error::Error;

pub struct AgentContextArgs<'a> {
    pub format: &'a str,
    pub since: Option<&'a str>,
    pub max_tokens: Option<usize>,
    pub smart: bool,
    pub fast: bool,
    pub config: &'a Config,
}

const DEFAULT_TOKEN_BUDGET: usize = 3000;

pub fn run(conn: &Connection, args: &AgentContextArgs) -> Result<(), Error> {
    if args.smart && args.fast {
        return Err(Error::Config(
            "cannot use both --smart and --fast".to_string(),
        ));
    }

    let git_repo = resolve_git_repo();
    let commands = fetch_project_commands(conn, &git_repo, args.since)?;

    if commands.is_empty() {
        println!("No RedTrail history for this directory yet.");
        return Ok(());
    }

    let sessions = group_by_agent_session(&commands);
    let recent = select_recent_sessions(&sessions, args.since);
    let analyses = analyze_sessions(recent);

    let git_state = git_state::collect_git_state();

    let use_smart = if args.fast {
        false
    } else if args.smart {
        true
    } else {
        args.config.llm.enabled
    };

    let output = if use_smart {
        match args.format {
            "json" => render_json(&git_repo, &analyses, &git_state),
            _ => smart::render_smart(&args.config.llm, &git_repo, &analyses, &git_state),
        }
    } else {
        match args.format {
            "json" => render_json(&git_repo, &analyses, &git_state),
            _ => fast::render_fast(&git_repo, &analyses, &git_state),
        }
    };

    let budget = args.max_tokens.unwrap_or(DEFAULT_TOKEN_BUDGET);
    let output = trim_to_budget(&output, budget);

    print!("{output}");
    Ok(())
}

fn resolve_git_repo() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.canonicalize().ok().or(Some(p)))
                    .and_then(|p| p.to_str().map(String::from))
            }
        })
}

fn fetch_project_commands(
    conn: &Connection,
    git_repo: &Option<String>,
    since: Option<&str>,
) -> Result<Vec<CommandRow>, Error> {
    let since_ts = match since {
        Some(dur) => Some(parse_duration_ago(dur)?),
        None => None,
    };

    db::get_commands(
        conn,
        &CommandFilter {
            git_repo: git_repo.as_deref(),
            since: since_ts,
            limit: Some(10000),
            ..Default::default()
        },
    )
}

fn group_by_agent_session(commands: &[CommandRow]) -> Vec<(String, Vec<CommandRow>)> {
    let mut groups: HashMap<String, Vec<CommandRow>> = HashMap::new();
    for cmd in commands {
        let key = cmd
            .agent_session_id
            .as_deref()
            .unwrap_or(&cmd.session_id)
            .to_string();
        groups.entry(key).or_default().push(cmd.clone());
    }
    let mut groups: Vec<(String, Vec<CommandRow>)> = groups.into_iter().collect();
    groups.sort_by(|a, b| {
        let max_a = a.1.iter().map(|c| c.timestamp_start).max().unwrap_or(0);
        let max_b = b.1.iter().map(|c| c.timestamp_start).max().unwrap_or(0);
        max_b.cmp(&max_a)
    });
    groups
}

fn select_recent_sessions(
    sessions: &[(String, Vec<CommandRow>)],
    since: Option<&str>,
) -> Vec<(String, Vec<CommandRow>)> {
    let limit = if since.is_some() { sessions.len() } else { 3 };
    sessions
        .iter()
        .take(limit)
        .map(|(id, cmds)| (id.clone(), cmds.clone()))
        .collect()
}

fn analyze_sessions(
    sessions: Vec<(String, Vec<CommandRow>)>,
) -> Vec<(String, Vec<CommandRow>, crate::core::analysis::AnalysisResult)> {
    sessions
        .into_iter()
        .map(|(id, cmds)| {
            let analysis = analyze_session(&cmds);
            (id, cmds, analysis)
        })
        .collect()
}

fn render_json(
    git_repo: &Option<String>,
    analyses: &[(String, Vec<CommandRow>, crate::core::analysis::AnalysisResult)],
    git_state: &git_state::GitState,
) -> String {
    let sessions: Vec<serde_json::Value> = analyses
        .iter()
        .map(|(session_id, cmds, a)| {
            let earliest = cmds.iter().map(|c| c.timestamp_start).min().unwrap_or(0);
            let meaningful: Vec<String> = filter::last_meaningful(cmds, 10)
                .iter()
                .map(|c| {
                    filter::format_command_line(c, git_repo.as_deref().unwrap_or("."))
                })
                .collect();
            serde_json::json!({
                "session_id": session_id,
                "started": earliest,
                "duration_seconds": a.duration_seconds,
                "total_commands": a.total_commands,
                "files_modified": &a.files_modified,
                "files_created": &a.files_created,
                "errors_total": a.total_errors,
                "errors_resolved": a.errors_resolved,
                "test_runs": a.test_runs,
                "tests_passed": a.tests_passed,
                "tests_failed": a.tests_failed,
                "meaningful_commands": meaningful,
            })
        })
        .collect();

    let val = serde_json::json!({
        "directory": git_repo,
        "branch": git_state.branch,
        "uncommitted_count": git_state.uncommitted_count,
        "last_commit_message": git_state.last_commit_message,
        "last_commit_time": git_state.last_commit_relative_time,
        "sessions": sessions,
    });

    serde_json::to_string_pretty(&val).unwrap_or_default() + "\n"
}

/// Trim output to fit within an approximate token budget.
/// Uses chars/4 as the token estimate.
pub fn trim_to_budget(output: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens * 4;
    if output.len() <= max_chars {
        return output.to_string();
    }
    let truncated = &output[..max_chars];
    if let Some(pos) = truncated.rfind("\n## ") {
        format!("{}\n\n*[Truncated to fit token budget]*\n", &output[..pos])
    } else {
        format!("{}...\n\n*[Truncated to fit token budget]*\n", truncated)
    }
}
