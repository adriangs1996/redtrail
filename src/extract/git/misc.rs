// Parse `git remote -v`, `git tag`, and `git stash list` output.

use std::collections::HashSet;

use crate::extract::types::{Extraction, NewEntity, TypedEntityData};

pub fn parse_remote(stdout: &str, repo: &str) -> Extraction {
    let mut seen: HashSet<String> = HashSet::new();
    let mut entities = Vec::new();

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: "name\turl (fetch|push)"
        let Some((remote_name, url_part)) = line.split_once('\t') else {
            continue;
        };
        let remote_name = remote_name.trim();
        if seen.contains(remote_name) {
            continue; // deduplicate fetch/push entries for same remote
        }
        seen.insert(remote_name.to_string());

        // Strip "(fetch)" or "(push)" suffix
        let url = url_part
            .rfind(" (")
            .map(|pos| url_part[..pos].trim())
            .unwrap_or(url_part.trim());

        entities.push(NewEntity {
            entity_type: "git_remote".into(),
            name: remote_name.to_string(),
            canonical_key: format!("{repo}:{remote_name}"),
            properties: None,
            typed_data: Some(TypedEntityData::GitRemote {
                repo: repo.into(),
                name: remote_name.to_string(),
                url: Some(url.to_string()),
            }),
            observation_context: None,
        });
    }

    Extraction {
        entities,
        relationships: Vec::new(),
    }
}

pub fn parse_tag(stdout: &str, repo: &str) -> Extraction {
    let entities = stdout
        .lines()
        .filter_map(|line| {
            let tag = line.trim();
            if tag.is_empty() {
                return None;
            }
            Some(NewEntity {
                entity_type: "git_tag".into(),
                name: tag.to_string(),
                canonical_key: format!("{repo}:{tag}"),
                properties: None,
                typed_data: Some(TypedEntityData::GitTag {
                    repo: repo.into(),
                    name: tag.to_string(),
                    commit_hash: None,
                }),
                observation_context: None,
            })
        })
        .collect();

    Extraction {
        entities,
        relationships: Vec::new(),
    }
}

pub fn parse_stash(stdout: &str, repo: &str) -> Extraction {
    let entities = stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // Format: "stash@{N}: message"
            let index_num = parse_stash_index(line);
            let message = line
                .find(": ")
                .map(|pos| line[pos + 2..].trim().to_string())
                .unwrap_or_else(|| line.to_string());

            let key = message
                .chars()
                .take(8)
                .collect::<String>()
                .to_lowercase()
                .replace(' ', "_");

            Some(NewEntity {
                entity_type: "git_stash".into(),
                name: message.clone(),
                canonical_key: format!("{repo}:{key}"),
                properties: None,
                typed_data: Some(TypedEntityData::GitStash {
                    repo: repo.into(),
                    index_num,
                    message,
                }),
                observation_context: None,
            })
        })
        .collect();

    Extraction {
        entities,
        relationships: Vec::new(),
    }
}

fn parse_stash_index(line: &str) -> i32 {
    line.find('{')
        .zip(line.find('}'))
        .and_then(|(open, close)| line[open + 1..close].parse().ok())
        .unwrap_or(0)
}
