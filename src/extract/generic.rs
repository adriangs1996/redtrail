// Generic fallback extractor — best-effort entity extraction for unrecognized commands.
//
// Extracts file paths, IP addresses, URLs, and ports from any command's stdout using
// regex patterns. Runs as the last-resort extractor after all domain-specific extractors.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::core::db::CommandRow;
use crate::extract::types::{Domain, DomainExtractor, ExtractError, Extraction, NewEntity};

// --- Compiled regex patterns ---

/// Absolute paths: must start with `/` followed by at least one word char or dot.
/// Relative paths: `./` or `../` prefix.
/// Optionally followed by `:line` or `:line:col`.
/// Preceded by start-of-string, whitespace, or common delimiter chars.
static FILE_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|\s|["'=(])((?:/[\w.\-/]+)|(?:\.\.?/[\w.\-/]+))(?::\d+(?::\d+)?)?"#)
        .unwrap()
});

/// IPv4 address — four dot-separated groups, each 1–3 digits.
/// Capture a leading character to validate boundary (space, start-of-line, comma, etc.).
/// We validate octets are 0-255 and reject version-number-like prefixes in code.
static IPV4_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Group 1: optional preceding boundary char (space/newline/tab/common punctuation)
    // Groups 2-5: the four octets
    Regex::new(r"(^|[\s,;(\[{])(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?:[^.\d]|$)").unwrap()
});

/// URLs starting with http:// or https://.
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://\S+").unwrap());

/// Port from a URL: `host:PORT/` or `host:PORT` at end of string / before whitespace.
static URL_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^:/\s]+:(\d+)").unwrap());

/// Contextual port mentions: "port 8080", "Listening on :8080", ":8080" at word boundary.
static CONTEXT_PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:port\s+|listening\s+on\s+:|on\s+:)(\d{1,5})\b").unwrap()
});

// --- Constants ---

const FILTERED_PATHS: &[&str] = &["/dev/null", "/dev/tty", "/dev/stdin", "/dev/stdout", "/dev/stderr"];

const TRAILING_PUNCT: &[char] = &[',', '.', ')', ']', ';', '\'', '"'];

// --- Public API ---

pub struct GenericExtractor;

impl DomainExtractor for GenericExtractor {
    fn domain(&self) -> Domain {
        Domain::Generic
    }

    fn can_handle(&self, _binary: &str, _subcommand: Option<&str>) -> bool {
        true
    }

    fn extract(&self, cmd: &CommandRow) -> Result<Extraction, ExtractError> {
        let raw = cmd.stdout.as_deref().unwrap_or("");
        if raw.trim().is_empty() {
            return Ok(Extraction::empty());
        }

        let text = strip_ansi(raw);

        // URLs must be collected first so we can exclude URL-embedded paths from
        // the file path extractor.
        let (url_entities, url_strings) = extract_urls(&text);
        let port_entities = extract_ports(&text, &url_strings);
        let ip_entities = extract_ips(&text);
        let file_entities = extract_files(&text, cmd.cwd.as_deref(), &url_strings);

        let mut seen_keys: HashSet<String> = HashSet::new();
        let mut entities: Vec<NewEntity> = Vec::new();

        for entity in file_entities
            .into_iter()
            .chain(ip_entities)
            .chain(url_entities)
            .chain(port_entities)
        {
            if seen_keys.insert(entity.canonical_key.clone()) {
                entities.push(entity);
            }
        }

        Ok(Extraction {
            entities,
            relationships: Vec::new(),
        })
    }
}

// --- Extraction helpers ---

fn extract_files(text: &str, cwd: Option<&str>, url_strings: &HashSet<String>) -> Vec<NewEntity> {
    let mut entities = Vec::new();

    for cap in FILE_PATH_RE.captures_iter(text) {
        let raw_path = cap[1].to_string();

        // Skip if this path appears inside a URL we already captured
        if url_strings.iter().any(|u| u.contains(&raw_path)) {
            continue;
        }

        // Skip filtered system paths
        if FILTERED_PATHS.iter().any(|fp| raw_path.starts_with(fp)) {
            continue;
        }

        // Resolve the canonical path (absolute vs relative)
        let canonical = if raw_path.starts_with('/') {
            raw_path.clone()
        } else {
            // Relative path — resolve against cwd if available
            match cwd {
                Some(base) => resolve_relative(base, &raw_path),
                None => raw_path.clone(),
            }
        };

        let name = canonical
            .rsplit('/')
            .next()
            .unwrap_or(&canonical)
            .to_string();

        entities.push(NewEntity {
            entity_type: "file".into(),
            name,
            canonical_key: canonical,
            properties: None,
            typed_data: None,
            observation_context: None,
        });
    }

    entities
}

fn extract_ips(text: &str) -> Vec<NewEntity> {
    let mut entities = Vec::new();

    for cap in IPV4_RE.captures_iter(text) {
        // Groups 2-5 are the octets (group 1 is the boundary char)
        let raw_octets = [&cap[2], &cap[3], &cap[4], &cap[5]];
        let ip = format!("{}.{}.{}.{}", raw_octets[0], raw_octets[1], raw_octets[2], raw_octets[3]);

        // Validate: all octets must be 0-255
        let all_valid = raw_octets.iter().all(|s| {
            s.parse::<u16>().map(|n| n <= 255).unwrap_or(false)
        });
        if !all_valid {
            continue;
        }

        // Exclude special-case addresses that are never meaningful entities
        if ip == "0.0.0.0" || ip == "255.255.255.255" {
            continue;
        }

        entities.push(NewEntity {
            entity_type: "ip_address".into(),
            name: ip.clone(),
            canonical_key: ip,
            properties: None,
            typed_data: None,
            observation_context: None,
        });
    }

    entities
}

/// Returns (url_entities, set_of_url_strings_for_filtering).
fn extract_urls(text: &str) -> (Vec<NewEntity>, HashSet<String>) {
    let mut entities = Vec::new();
    let mut url_strings: HashSet<String> = HashSet::new();

    for m in URL_RE.find_iter(text) {
        let mut url = m.as_str().to_string();
        // Trim trailing punctuation that is almost certainly not part of the URL
        while url.ends_with(|c| TRAILING_PUNCT.contains(&c)) {
            url.pop();
        }

        url_strings.insert(url.clone());

        entities.push(NewEntity {
            entity_type: "url".into(),
            name: url.clone(),
            canonical_key: url,
            properties: None,
            typed_data: None,
            observation_context: None,
        });
    }

    (entities, url_strings)
}

fn extract_ports(text: &str, url_strings: &HashSet<String>) -> Vec<NewEntity> {
    let mut entities = Vec::new();

    // Ports embedded in URLs (e.g., http://localhost:3000/path)
    for url in url_strings {
        if let Some(cap) = URL_PORT_RE.captures(url)
            && let Some(port) = parse_valid_port(&cap[1])
        {
            entities.push(port_entity(port));
        }
    }

    // Contextual port mentions in text
    for cap in CONTEXT_PORT_RE.captures_iter(text) {
        if let Some(port) = parse_valid_port(&cap[1]) {
            entities.push(port_entity(port));
        }
    }

    entities
}

// --- Utility functions ---

fn strip_ansi(text: &str) -> String {
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

/// Resolve a relative path (starting with `./` or `../`) against a base directory.
fn resolve_relative(base: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = base.trim_end_matches('/').split('/').collect();

    for segment in rel.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    parts.join("/")
}

/// Parse a port string, returning `Some(port)` only if 1 ≤ port ≤ 65535.
fn parse_valid_port(s: &str) -> Option<u16> {
    s.parse::<u32>().ok().and_then(|n| {
        if (1..=65535).contains(&n) {
            Some(n as u16)
        } else {
            None
        }
    })
}

fn port_entity(port: u16) -> NewEntity {
    NewEntity {
        entity_type: "port".into(),
        name: port.to_string(),
        canonical_key: format!("port:{port}"),
        properties: None,
        typed_data: None,
        observation_context: None,
    }
}

// --- Unit tests for internal helpers ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_dot_slash() {
        assert_eq!(
            resolve_relative("/home/user/project", "./src/lib.rs"),
            "/home/user/project/src/lib.rs"
        );
    }

    #[test]
    fn resolve_relative_parent() {
        assert_eq!(
            resolve_relative("/home/user/project", "../other/file.rs"),
            "/home/user/other/file.rs"
        );
    }

    #[test]
    fn parse_valid_port_accepts_range() {
        assert_eq!(parse_valid_port("1"), Some(1));
        assert_eq!(parse_valid_port("65535"), Some(65535));
        assert_eq!(parse_valid_port("3000"), Some(3000));
    }

    #[test]
    fn parse_valid_port_rejects_out_of_range() {
        assert_eq!(parse_valid_port("0"), None);
        assert_eq!(parse_valid_port("65536"), None);
        assert_eq!(parse_valid_port("99999"), None);
    }
}
