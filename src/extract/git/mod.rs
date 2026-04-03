// Git domain extractor — dispatches git command output to subcommand parsers.

mod branch;
mod diff;
mod log;
mod misc;
mod status;

use crate::core::db::CommandRow;
use crate::extract::types::{
    Domain, DomainExtractor, ExtractError, Extraction, NewEntity, NewRelationship,
};

pub use branch::parse_branch;
pub use diff::parse_diff;
pub use log::parse_log;
pub use misc::{parse_remote, parse_stash, parse_tag};
pub use status::parse_status;

pub struct GitExtractor;

impl DomainExtractor for GitExtractor {
    fn domain(&self) -> Domain {
        Domain::Git
    }

    fn can_handle(&self, binary: &str, _subcommand: Option<&str>) -> bool {
        binary == "git"
    }

    fn extract(&self, cmd: &CommandRow) -> Result<Extraction, ExtractError> {
        let raw_stdout = cmd.stdout.as_deref().unwrap_or("");
        let stdout = strip_ansi(raw_stdout);
        if stdout.trim().is_empty() {
            return Ok(Extraction::empty());
        }
        let repo = cmd.git_repo.as_deref().unwrap_or("unknown");

        let mut extraction = match cmd.command_subcommand.as_deref() {
            Some("status") => parse_status(&stdout, repo),
            Some("log") => parse_log(&stdout, repo),
            Some("diff" | "show") => parse_diff(&stdout, repo),
            Some("branch") => parse_branch(&stdout, repo),
            Some("remote") => parse_remote(&stdout, repo),
            Some("tag") => parse_tag(&stdout, repo),
            Some("stash") => parse_stash(&stdout, repo),
            _ => Extraction::empty(),
        };

        // Always add the repo entity and belongs_to relationships when repo is known
        if repo != "unknown" {
            extraction.entities.push(repo_entity(repo));

            let indices: Vec<usize> = extraction
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    matches!(
                        e.entity_type.as_str(),
                        "git_file" | "git_branch" | "git_commit" | "git_tag" | "git_stash"
                    )
                })
                .map(|(i, _)| i)
                .collect();

            let mut new_rels: Vec<NewRelationship> = indices
                .iter()
                .map(|&i| {
                    let entity = &extraction.entities[i];
                    NewRelationship {
                        source_type: entity.entity_type.clone(),
                        source_canonical_key: entity.canonical_key.clone(),
                        target_type: "git_repo".into(),
                        target_canonical_key: repo.into(),
                        relation_type: "belongs_to".into(),
                        properties: None,
                    }
                })
                .collect();

            extraction.relationships.append(&mut new_rels);
        }

        Ok(extraction)
    }
}

// --- Shared helpers used across submodules ---

/// Strip ANSI escape sequences (ESC+[+...m color codes, cursor movement codes)
/// while preserving all other characters including tabs and newlines.
///
/// The `strip_ansi_escapes` crate uses a VTE parser which treats tabs as cursor
/// movement and strips them — that breaks git output which uses tabs as separators
/// (e.g. `git remote -v`) and for indentation (e.g. `git status` long format).
pub(super) fn strip_ansi(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    chars.next(); // consume '['
                    for sc in chars.by_ref() {
                        if sc.is_ascii_alphabetic() || sc == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next(); // consume ']'
                    for sc in chars.by_ref() {
                        if sc == '\x07' || sc == '\u{9C}' {
                            break;
                        }
                        if sc == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub(super) fn repo_entity(repo: &str) -> NewEntity {
    NewEntity {
        entity_type: "git_repo".into(),
        name: repo.rsplit('/').next().unwrap_or(repo).into(),
        canonical_key: repo.into(),
        properties: None,
        typed_data: None,
        observation_context: None,
    }
}

pub(super) fn file_entity(repo: &str, path: &str, status: &str) -> NewEntity {
    let path = path.trim().to_string();
    NewEntity {
        entity_type: "git_file".into(),
        name: path.clone(),
        canonical_key: format!("{repo}:{path}"),
        properties: None,
        typed_data: Some(crate::extract::types::TypedEntityData::GitFile {
            repo: repo.into(),
            path: path.clone(),
            status: Some(status.into()),
            insertions: None,
            deletions: None,
        }),
        observation_context: None,
    }
}

pub(super) fn file_entity_with_stats(
    repo: &str,
    path: &str,
    insertions: Option<i32>,
    deletions: Option<i32>,
) -> NewEntity {
    let path = path.trim().to_string();
    NewEntity {
        entity_type: "git_file".into(),
        name: path.clone(),
        canonical_key: format!("{repo}:{path}"),
        properties: None,
        typed_data: Some(crate::extract::types::TypedEntityData::GitFile {
            repo: repo.into(),
            path: path.clone(),
            status: Some("modified".into()),
            insertions,
            deletions,
        }),
        observation_context: None,
    }
}
