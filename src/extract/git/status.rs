// Parse `git status` output into git_file entities.

use crate::extract::types::Extraction;
use super::file_entity;

pub fn parse_status(stdout: &str, repo: &str) -> Extraction {
    // Determine format: short format lines start with two status chars then a space.
    // Long format lines use tab-indentation like "\tmodified:   path".
    let is_short = stdout.lines().any(is_short_status_line);

    let entities = if is_short {
        parse_status_short(stdout, repo)
    } else {
        parse_status_long(stdout, repo)
    };

    Extraction {
        entities,
        relationships: Vec::new(),
    }
}

fn is_short_status_line(line: &str) -> bool {
    // Short format: exactly 2 status chars then a space, e.g. " M ", "?? ", "A  "
    if line.len() < 3 {
        return false;
    }
    let bytes = line.as_bytes();
    let valid = b"MADRCU? ";
    valid.contains(&bytes[0]) && valid.contains(&bytes[1]) && bytes[2] == b' '
}

fn parse_status_short(stdout: &str, repo: &str) -> Vec<crate::extract::types::NewEntity> {
    stdout
        .lines()
        .filter(|line| line.len() >= 4)
        .filter_map(|line| {
            let xy = &line[..2];
            let path = line[3..].trim();
            if path.is_empty() {
                return None;
            }
            let status = short_code_to_status(xy)?;
            Some(file_entity(repo, path, status))
        })
        .collect()
}

fn short_code_to_status(xy: &str) -> Option<&'static str> {
    let x = xy.as_bytes().first().copied().unwrap_or(b' ');
    let y = xy.as_bytes().get(1).copied().unwrap_or(b' ');
    match (x, y) {
        (b'?', b'?') => Some("untracked"),
        (b'A', _) => Some("staged"),
        (b'D', _) | (_, b'D') => Some("deleted"),
        (b'R', _) => Some("renamed"),
        (b'M', _) | (_, b'M') => Some("modified"),
        (b'C', _) => Some("copied"),
        (b'U', _) | (_, b'U') => Some("conflict"),
        _ => None,
    }
}

fn parse_status_long(stdout: &str, repo: &str) -> Vec<crate::extract::types::NewEntity> {
    let mut entities = Vec::new();
    let mut in_untracked = false;

    for line in stdout.lines() {
        if line.starts_with("Untracked files:") {
            in_untracked = true;
            continue;
        }
        if line.starts_with("Changes") || line.starts_with("nothing") {
            in_untracked = false;
        }

        if in_untracked {
            if line.starts_with("  (") || line.trim().is_empty() {
                continue;
            }
            if let Some(path) = line.strip_prefix('\t') {
                let path = path.trim();
                if !path.is_empty() {
                    entities.push(file_entity(repo, path, "untracked"));
                }
            }
        } else if let Some(stripped) = line.strip_prefix('\t') {
            let stripped = stripped.trim();
            for &(prefix, status) in LONG_FORMAT_PREFIXES {
                if let Some(rest) = stripped.strip_prefix(prefix) {
                    let path = rest.trim();
                    // For renames: "old -> new", take the new name
                    let path = path
                        .find(" -> ")
                        .map(|p| path[p + 4..].trim())
                        .unwrap_or(path);
                    if !path.is_empty() {
                        entities.push(file_entity(repo, path, status));
                    }
                    break;
                }
            }
        }
    }

    entities
}

const LONG_FORMAT_PREFIXES: &[(&str, &str)] = &[
    ("modified:", "modified"),
    ("both modified:", "modified"),
    ("new file:", "staged"),
    ("added by us:", "staged"),
    ("added by them:", "staged"),
    ("deleted:", "deleted"),
    ("deleted by us:", "deleted"),
    ("deleted by them:", "deleted"),
    ("renamed:", "renamed"),
    ("copied:", "copied"),
    ("both added:", "conflict"),
    ("both deleted:", "conflict"),
];
