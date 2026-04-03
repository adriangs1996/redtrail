// Parse `git branch` output into git_branch entities.

use crate::extract::types::{Extraction, NewEntity, TypedEntityData};

pub fn parse_branch(stdout: &str, repo: &str) -> Extraction {
    let entities = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let is_current = line.starts_with('*');
            let name_raw = if is_current { &line[1..] } else { line }.trim();

            let (is_remote, name) = if let Some(stripped) = name_raw.strip_prefix("remotes/") {
                (true, stripped.to_string())
            } else {
                (false, name_raw.to_string())
            };

            let remote_name = if is_remote {
                name.split('/').next().map(String::from)
            } else {
                None
            };

            NewEntity {
                entity_type: "git_branch".into(),
                name: name.clone(),
                canonical_key: format!("{repo}:{name}:{is_remote}"),
                properties: None,
                typed_data: Some(TypedEntityData::GitBranch {
                    repo: repo.into(),
                    name,
                    is_remote,
                    remote_name,
                    upstream: None,
                    ahead: None,
                    behind: None,
                    last_commit_hash: None,
                }),
                observation_context: None,
            }
        })
        .collect();

    Extraction {
        entities,
        relationships: Vec::new(),
    }
}
