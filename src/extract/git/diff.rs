// Parse `git diff` / `git show` output into git_file entities with insertion/deletion counts.

use crate::extract::types::Extraction;
use super::file_entity_with_stats;

pub fn parse_diff(stdout: &str, repo: &str) -> Extraction {
    // Try --stat format first: lines contain " | " and are not diff headers.
    let stat_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains(" | ") && !l.trim().starts_with("diff"))
        .collect();

    let entities = if !stat_lines.is_empty() {
        parse_diff_stat(stat_lines, repo)
    } else {
        parse_diff_full(stdout, repo)
    };

    Extraction {
        entities,
        relationships: Vec::new(),
    }
}

fn parse_diff_stat(lines: Vec<&str>, repo: &str) -> Vec<crate::extract::types::NewEntity> {
    lines
        .iter()
        .filter_map(|line| {
            let pipe_pos = line.find(" | ")?;
            let path = line[..pipe_pos].trim();
            // Skip the summary line "N files changed, ..."
            if path.is_empty() || path.contains("file") {
                return None;
            }
            let rest = &line[pipe_pos + 3..];
            let (insertions, deletions) = parse_stat_counts(rest);
            Some(file_entity_with_stats(repo, path, insertions, deletions))
        })
        .collect()
}

fn parse_diff_full(stdout: &str, repo: &str) -> Vec<crate::extract::types::NewEntity> {
    stdout
        .lines()
        .filter_map(|line| {
            // "diff --git a/path b/path"
            let rest = line.strip_prefix("diff --git ")?;
            let b_part = rest.split_once(" b/")?.1;
            let path = b_part.trim();
            if path.is_empty() {
                return None;
            }
            Some(file_entity_with_stats(repo, path, None, None))
        })
        .collect()
}

fn parse_stat_counts(s: &str) -> (Option<i32>, Option<i32>) {
    let plus = s.chars().filter(|&c| c == '+').count() as i32;
    let minus = s.chars().filter(|&c| c == '-').count() as i32;
    (
        if plus > 0 { Some(plus) } else { None },
        if minus > 0 { Some(minus) } else { None },
    )
}
