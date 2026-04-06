// Smart mode renderer: LLM-powered summarization with graceful fallback.

use crate::config::LlmConfig;
use crate::core::analysis::AnalysisResult;
use crate::core::db::CommandRow;
use crate::core::errors::detect_error_fix_sequences_filtered;
use crate::core::fmt::markdown;

use super::git_state::GitState;
use super::llm;

pub fn render_smart(
    config: &LlmConfig,
    project_root: &Option<String>,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    git_state: &GitState,
) -> String {
    let root = project_root.as_deref().unwrap_or(".");
    let mut out = String::new();

    out.push_str("# Project Context (RedTrail)\n\n");
    render_where_you_left_off(&mut out, config, sessions, root);
    render_open_work(&mut out, sessions, git_state);
    render_recent_decisions(&mut out, config, sessions, root);
    render_known_issues_and_fixes(&mut out, config, sessions);
    render_project_structure(&mut out, sessions);

    out
}

fn render_where_you_left_off(
    out: &mut String,
    config: &LlmConfig,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    root: &str,
) {
    out.push_str("## Where You Left Off\n");
    if let Some((_, cmds, _)) = sessions.first() {
        let summary = llm::summarize_session(config, cmds, root);
        out.push_str(&summary);
    } else {
        out.push_str("No recent sessions found.");
    }
    out.push_str("\n\n");
}

fn render_open_work(
    out: &mut String,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    git_state: &GitState,
) {
    out.push_str("## Open Work\n");

    if git_state.uncommitted_count > 0 {
        out.push_str(&format!(
            "- {} uncommitted file(s)\n",
            git_state.uncommitted_count,
        ));
    }

    // Check for failing tests in most recent session
    if let Some((_, _, analysis)) = sessions.first() {
        if analysis.tests_failed > 0 {
            out.push_str(&format!("- {} test(s) failing\n", analysis.tests_failed));
        }
        let unresolved_errors = analysis.total_errors.saturating_sub(analysis.errors_resolved);
        if unresolved_errors > 0 {
            out.push_str(&format!("- {} unresolved error(s)\n", unresolved_errors));
        }
    }

    if git_state.uncommitted_count == 0
        && sessions
            .first()
            .map(|(_, _, a)| a.tests_failed == 0 && a.total_errors == a.errors_resolved)
            .unwrap_or(true)
    {
        out.push_str("- Clean state: no uncommitted changes, no failing tests\n");
    }
    out.push('\n');
}

fn render_recent_decisions(
    out: &mut String,
    config: &LlmConfig,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
    root: &str,
) {
    let all_cmds: Vec<CommandRow> = sessions
        .iter()
        .flat_map(|(_, cmds, _)| cmds.iter().cloned())
        .collect();

    let decisions = llm::summarize_decisions(config, &all_cmds, root);
    if decisions.is_empty() {
        return;
    }

    out.push_str("## Recent Decisions\n");
    out.push_str(&decisions);
    out.push_str("\n\n");
}

fn render_known_issues_and_fixes(
    out: &mut String,
    config: &LlmConfig,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
) {
    let all_cmds: Vec<CommandRow> = sessions
        .iter()
        .flat_map(|(_, cmds, _)| cmds.iter().cloned())
        .collect();

    let filtered = detect_error_fix_sequences_filtered(&all_cmds);
    let resolved: Vec<_> = filtered.into_iter().filter(|s| s.resolved).collect();

    if resolved.is_empty() {
        return;
    }

    let llm_summary = llm::summarize_error_fixes(config, &resolved);
    if !llm_summary.is_empty() {
        out.push_str("## Known Issues & Fixes\n");
        out.push_str(&llm_summary);
        out.push_str("\n\n");
        return;
    }

    // Fallback: render heuristic version
    out.push_str("## Known Issues & Fixes\n");
    for seq in &resolved {
        let fix = seq
            .resolution_command
            .as_deref()
            .unwrap_or("(manual fix)");
        out.push_str(&format!(
            "- `{}` -> `{}`\n",
            markdown::escape(&seq.failing_command),
            markdown::escape(fix),
        ));
    }
    out.push('\n');
}

fn render_project_structure(
    out: &mut String,
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
) {
    // Heuristic-based for v1: detect language from file extensions
    let all_files: Vec<&str> = sessions
        .iter()
        .flat_map(|(_, _, a)| {
            a.files_modified
                .iter()
                .chain(a.files_created.iter())
                .map(String::as_str)
        })
        .collect();

    if all_files.is_empty() {
        return;
    }

    let language = detect_language(&all_files);
    let build_tool = detect_build_tool(sessions);

    if language.is_none() && build_tool.is_none() {
        return;
    }

    out.push_str("## Project Structure\n");
    let mut parts = Vec::new();
    if let Some(lang) = language {
        parts.push(format!("Language: {lang}"));
    }
    if let Some(tool) = build_tool {
        parts.push(format!("Build: {tool}"));
    }
    out.push_str(&parts.join(". "));
    out.push_str("\n\n");
}

fn detect_language(files: &[&str]) -> Option<&'static str> {
    let ext_counts: std::collections::HashMap<&str, usize> = files
        .iter()
        .filter_map(|f| f.rsplit('.').next())
        .fold(std::collections::HashMap::new(), |mut acc, ext| {
            *acc.entry(ext).or_insert(0) += 1;
            acc
        });

    ext_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .and_then(|(ext, _)| match ext {
            "rs" => Some("Rust"),
            "py" => Some("Python"),
            "ts" | "tsx" => Some("TypeScript"),
            "js" | "jsx" => Some("JavaScript"),
            "go" => Some("Go"),
            "java" => Some("Java"),
            "rb" => Some("Ruby"),
            "cpp" | "cc" | "cxx" => Some("C++"),
            "c" => Some("C"),
            _ => None,
        })
}

fn detect_build_tool(
    sessions: &[(String, Vec<CommandRow>, AnalysisResult)],
) -> Option<&'static str> {
    for (_, _, analysis) in sessions {
        for binary in analysis.binary_stats.keys() {
            match binary.as_str() {
                "cargo" => return Some("cargo"),
                "npm" | "npx" => return Some("npm"),
                "yarn" => return Some("yarn"),
                "pnpm" => return Some("pnpm"),
                "go" => return Some("go"),
                "make" => return Some("make"),
                "pip" | "pip3" => return Some("pip"),
                _ => {}
            }
        }
    }
    None
}
