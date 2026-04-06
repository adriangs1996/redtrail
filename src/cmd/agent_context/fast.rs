// Fast mode renderer: heuristic-only, no LLM calls. Target: <500ms.

use crate::core::analysis::AnalysisResult;
use crate::core::db::CommandRow;
use crate::core::errors::detect_error_fix_sequences_filtered;
use crate::core::fmt::ascii::format_relative_time;
use crate::core::fmt::markdown;
use crate::core::fmt::paths::to_relative;

use super::filter;
use super::git_state::GitState;

pub fn render_fast(
    project_root: &Option<String>,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    git_state: &GitState,
) -> String {
    let root = project_root.as_deref().unwrap_or(".");
    let mut out = String::new();

    out.push_str("# Project Context (RedTrail)\n\n");
    render_last_session_quick_view(&mut out, sessions, root);
    render_session_activity(&mut out, sessions, root);
    render_git_state_section(&mut out, git_state);
    render_known_fixes(&mut out, sessions, root);
    render_unresolved_issues(&mut out, sessions, root);

    out
}

fn render_last_session_quick_view(
    out: &mut String,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    root: &str,
) {
    let Some((_, cmds, analysis)) = sessions.first() else {
        return;
    };
    let dir = analysis
        .directory
        .as_deref()
        .map(|d| to_relative(d, root))
        .unwrap_or(".");
    let earliest_ts = cmds.iter().map(|c| c.timestamp_start).min().unwrap_or(0);
    let relative_time = format_relative_time(earliest_ts);

    out.push_str("## Last Session\n");
    out.push_str(&format!(
        "**{}** | {} commands | {} errors | started {}\n",
        markdown::escape(dir),
        analysis.total_commands,
        analysis.total_errors,
        relative_time,
    ));

    // Last 3 file writes/edits
    let writes = last_file_writes(cmds, root, 3);
    if !writes.is_empty() {
        out.push_str(&format!("Last writes: {}\n", writes.join(", ")));
    }

    // Last build/test result
    if let Some(result) = last_build_test_result(cmds) {
        out.push_str(&format!("Last build: {result}\n"));
    }
    out.push('\n');
}

fn last_file_writes(cmds: &[CommandRow], root: &str, n: usize) -> Vec<String> {
    let mut writes: Vec<&CommandRow> = cmds
        .iter()
        .filter(|c| {
            c.tool_name
                .as_deref()
                .is_some_and(|t| t == "Write" || t == "Edit" || t == "NotebookEdit")
        })
        .collect();
    writes.sort_by_key(|c| c.timestamp_start);
    writes
        .iter()
        .rev()
        .take(n)
        .map(|c| {
            let path = c
                .command_raw
                .split_once(' ')
                .map(|(_, rest)| rest)
                .unwrap_or(&c.command_raw)
                .trim();
            to_relative(path, root).to_string()
        })
        .collect()
}

fn last_build_test_result(cmds: &[CommandRow]) -> Option<String> {
    use crate::core::classify::{classify_command, CommandCategory};
    let mut sorted: Vec<&CommandRow> = cmds.iter().collect();
    sorted.sort_by_key(|c| c.timestamp_start);
    sorted
        .iter()
        .rev()
        .find(|c| {
            let cat = classify_command(
                c.command_binary.as_deref().unwrap_or(""),
                c.command_subcommand.as_deref(),
                c.tool_name.as_deref(),
            );
            cat == CommandCategory::TestRun || cat == CommandCategory::Build
        })
        .map(|c| {
            let icon = if c.exit_code == Some(0) { "✓" } else { "✗" };
            format!("{icon} {}", c.command_raw.trim())
        })
}

fn render_session_activity(
    out: &mut String,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    root: &str,
) {
    if sessions.is_empty() {
        return;
    }
    out.push_str("## Session Activity\n\n");
    for (i, (_id, cmds, analysis)) in sessions.iter().enumerate() {
        let earliest = cmds.iter().map(|c| c.timestamp_start).min().unwrap_or(0);
        let relative_time = format_relative_time(earliest);
        let duration = format_duration_human(analysis.duration_seconds);

        out.push_str(&format!(
            "### Session {} ({}, {})\n",
            i + 1,
            relative_time,
            duration,
        ));

        let meaningful = filter::last_meaningful(cmds, 10);
        for cmd in &meaningful {
            out.push_str(&format!("{}\n", filter::format_command_line(cmd, root)));
        }
        out.push('\n');
    }
}

fn render_git_state_section(out: &mut String, git: &GitState) {
    out.push_str("## Git State\n");
    if let Some(branch) = &git.branch {
        out.push_str(&format!("Branch: {branch}\n"));
    }
    if git.uncommitted_count > 0 {
        out.push_str(&format!("Uncommitted: {} files\n", git.uncommitted_count));
    }
    if let Some(msg) = &git.last_commit_message {
        let time = git
            .last_commit_relative_time
            .as_deref()
            .unwrap_or("unknown");
        out.push_str(&format!("Last commit: \"{msg}\" ({time})\n"));
    }
    out.push('\n');
}

fn render_known_fixes(
    out: &mut String,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    _root: &str,
) {
    let all_cmds: Vec<CommandRow> = sessions
        .iter()
        .flat_map(|(_, cmds, _)| cmds.iter().cloned())
        .collect();
    let filtered = detect_error_fix_sequences_filtered(&all_cmds);
    let resolved: Vec<_> = filtered.iter().filter(|s| s.resolved).collect();

    if resolved.is_empty() {
        return;
    }

    out.push_str("## Known Fixes\n");
    // Deduplicate by failing_command
    let mut seen = std::collections::HashMap::new();
    for seq in &resolved {
        let count = seen.entry(seq.failing_command.clone()).or_insert(0usize);
        *count += 1;
    }
    let mut deduped: Vec<_> = seen.into_iter().collect();
    deduped.sort_by(|a, b| b.1.cmp(&a.1));

    for (failing_cmd, count) in &deduped {
        let seq = resolved
            .iter()
            .find(|s| &s.failing_command == failing_cmd)
            .unwrap();
        let snippet = if seq.stderr_snippet.is_empty() {
            seq.error_message.clone()
        } else {
            seq.stderr_snippet.lines().next().unwrap_or("").to_string()
        };
        let fix = seq
            .resolution_command
            .as_deref()
            .unwrap_or("(manual fix)");
        out.push_str(&format!(
            "- `{}`: \"{}\" -> `{}` ({}x)\n",
            markdown::escape(failing_cmd),
            markdown::escape(&snippet),
            markdown::escape(fix),
            count,
        ));
    }
    out.push('\n');
}

fn render_unresolved_issues(
    out: &mut String,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    _root: &str,
) {
    let all_cmds: Vec<CommandRow> = sessions
        .iter()
        .flat_map(|(_, cmds, _)| cmds.iter().cloned())
        .collect();
    let filtered = detect_error_fix_sequences_filtered(&all_cmds);
    let unresolved: Vec<_> = filtered.iter().filter(|s| !s.resolved).collect();

    if unresolved.is_empty() {
        return;
    }

    out.push_str("## Unresolved Issues\n");
    for seq in &unresolved {
        let snippet = if seq.stderr_snippet.is_empty() {
            seq.error_message.clone()
        } else {
            seq.stderr_snippet.lines().next().unwrap_or("").to_string()
        };
        out.push_str(&format!(
            "- `{}`: {}\n",
            markdown::escape(&seq.failing_command),
            markdown::escape(&snippet),
        ));
    }
    out.push('\n');
}

fn format_duration_human(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}
