// Parse `git log` output into git_commit and person entities.

use crate::extract::types::{Extraction, NewEntity, NewRelationship, TypedEntityData};

pub fn parse_log(stdout: &str, repo: &str) -> Extraction {
    // Default format starts with "commit <40-char hash>"; oneline has short hashes.
    let is_default = stdout
        .lines()
        .any(|line| line.starts_with("commit ") && line.len() >= 47);

    if is_default {
        parse_log_default(stdout, repo)
    } else {
        parse_log_oneline(stdout, repo)
    }
}

fn parse_log_default(stdout: &str, repo: &str) -> Extraction {
    let mut entities: Vec<NewEntity> = Vec::new();
    let mut relationships: Vec<NewRelationship> = Vec::new();

    let mut current_hash: Option<String> = None;
    let mut current_author_name: Option<String> = None;
    let mut current_author_email: Option<String> = None;
    let mut message_lines: Vec<String> = Vec::new();
    let mut in_message = false;

    for line in stdout.lines() {
        if line.starts_with("commit ") && line.len() >= 47 {
            flush_commit(
                current_hash.take(),
                current_author_name.take(),
                current_author_email.take(),
                &message_lines,
                repo,
                &mut entities,
                &mut relationships,
            );
            message_lines.clear();
            in_message = false;
            current_hash = Some(line[7..].trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Author: ") {
            let (name, email) = parse_author(rest);
            current_author_name = name;
            current_author_email = email;
        } else if line.starts_with("Date: ") || line.starts_with("Merge: ") {
            // skip — we don't parse these fields currently
        } else if line.is_empty() && current_hash.is_some() {
            in_message = true;
        } else if in_message {
            if let Some(msg) = line.strip_prefix("    ") {
                message_lines.push(msg.to_string());
            }
        }
    }

    flush_commit(
        current_hash.take(),
        current_author_name.take(),
        current_author_email.take(),
        &message_lines,
        repo,
        &mut entities,
        &mut relationships,
    );

    Extraction {
        entities,
        relationships,
    }
}

fn flush_commit(
    hash: Option<String>,
    author_name: Option<String>,
    author_email: Option<String>,
    message_lines: &[String],
    repo: &str,
    entities: &mut Vec<NewEntity>,
    relationships: &mut Vec<NewRelationship>,
) {
    let Some(hash) = hash else { return };

    let message = {
        let joined = message_lines
            .iter()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join("\n");
        let trimmed = joined.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    };
    let short_hash = hash.get(..7).map(String::from);

    entities.push(NewEntity {
        entity_type: "git_commit".into(),
        name: short_hash.clone().unwrap_or_else(|| hash.clone()),
        canonical_key: format!("{repo}:{hash}"),
        properties: None,
        typed_data: Some(TypedEntityData::GitCommit {
            repo: repo.into(),
            hash: hash.clone(),
            short_hash,
            author_name: author_name.clone(),
            author_email: author_email.clone(),
            message,
            committed_at: None,
        }),
        observation_context: None,
    });

    if let Some(email) = &author_email {
        // Deduplicate person entities by canonical key
        if !entities.iter().any(|e| e.canonical_key == *email) {
            entities.push(NewEntity {
                entity_type: "person".into(),
                name: author_name.clone().unwrap_or_else(|| email.clone()),
                canonical_key: email.clone(),
                properties: None,
                typed_data: None,
                observation_context: None,
            });
        }
        relationships.push(NewRelationship {
            source_type: "git_commit".into(),
            source_canonical_key: format!("{repo}:{hash}"),
            target_type: "person".into(),
            target_canonical_key: email.clone(),
            relation_type: "authored_by".into(),
            properties: None,
        });
    }
}

fn parse_author(s: &str) -> (Option<String>, Option<String>) {
    if let (Some(open), Some(close)) = (s.find('<'), s.find('>')) {
        let name = s[..open].trim().to_string();
        let email = s[open + 1..close].trim().to_string();
        (
            if name.is_empty() { None } else { Some(name) },
            if email.is_empty() { None } else { Some(email) },
        )
    } else {
        let name = s.trim().to_string();
        (if name.is_empty() { None } else { Some(name) }, None)
    }
}

fn parse_log_oneline(stdout: &str, repo: &str) -> Extraction {
    let entities = stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (hash, message) = line.split_once(' ').unwrap_or((line, ""));
            if hash.is_empty() {
                return None;
            }
            Some(NewEntity {
                entity_type: "git_commit".into(),
                name: hash.to_string(),
                canonical_key: format!("{repo}:{hash}"),
                properties: None,
                typed_data: Some(TypedEntityData::GitCommit {
                    repo: repo.into(),
                    hash: hash.to_string(),
                    short_hash: Some(hash.to_string()),
                    author_name: None,
                    author_email: None,
                    message: if message.is_empty() {
                        None
                    } else {
                        Some(message.trim().to_string())
                    },
                    committed_at: None,
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
