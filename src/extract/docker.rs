// Docker domain extractor — parses docker command output into DockerContainer, DockerImage, etc.

use crate::core::db::CommandRow;
use crate::extract::types::{
    Domain, DomainExtractor, ExtractError, Extraction, NewEntity, TypedEntityData,
};

pub struct DockerExtractor;

impl DomainExtractor for DockerExtractor {
    fn domain(&self) -> Domain {
        Domain::Docker
    }

    fn can_handle(&self, binary: &str, _subcommand: Option<&str>) -> bool {
        matches!(binary, "docker" | "docker-compose" | "podman")
    }

    fn extract(&self, cmd: &CommandRow) -> Result<Extraction, ExtractError> {
        let raw_stdout = cmd.stdout.as_deref().unwrap_or("");
        let stdout = strip_ansi(raw_stdout);
        if stdout.trim().is_empty() {
            return Ok(Extraction::empty());
        }

        let binary = cmd.command_binary.as_deref().unwrap_or("docker");
        let subcommand = cmd.command_subcommand.as_deref().unwrap_or("");

        let entities = match (binary, subcommand) {
            (_, "ps") if is_compose(binary) => parse_compose_ps(&stdout),
            (_, "ps") => parse_docker_ps(&stdout),
            (_, "images" | "image ls" | "image list") => parse_docker_images(&stdout),
            (_, "build") => parse_docker_build(&stdout),
            (_, "compose ps") => parse_compose_ps(&stdout),
            _ => Vec::new(),
        };

        Ok(Extraction {
            entities,
            relationships: Vec::new(),
        })
    }
}

fn is_compose(binary: &str) -> bool {
    binary == "docker-compose"
}

// --- docker ps ---

/// Parse `docker ps` tabular output.
///
/// Docker ps uses fixed-width columns whose boundaries are defined by the header line.
/// We locate the start of each column from the header, then slice each data row at
/// those positions. This is more robust than splitting on whitespace because fields
/// like COMMAND and NAMES can contain spaces.
fn parse_docker_ps(stdout: &str) -> Vec<NewEntity> {
    let mut lines = stdout.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // Column names we care about (exact header text, case-sensitive as Docker emits them).
    let col_container_id = find_col_start(header, "CONTAINER ID");
    let col_image = find_col_start(header, "IMAGE");
    let col_command = find_col_start(header, "COMMAND");
    let col_status = find_col_start(header, "STATUS");
    let col_ports = find_col_start(header, "PORTS");
    let col_names = find_col_start(header, "NAMES");

    // Required columns: CONTAINER ID, IMAGE, STATUS, NAMES. COMMAND is optional but
    // used to tightly bound the IMAGE field so multi-byte characters in COMMAND don't
    // bleed back into our extraction.
    let (col_container_id, col_image, col_status, col_names) =
        match (col_container_id, col_image, col_status, col_names) {
            (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
            _ => return Vec::new(),
        };

    // IMAGE ends at COMMAND if present, else at STATUS.
    let col_image_end = col_command.unwrap_or(col_status);

    lines
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let name = extract_col(line, col_names, None).trim().to_string();
            if name.is_empty() {
                return None;
            }
            let container_id =
                non_empty(extract_col(line, col_container_id, Some(col_image)).trim());
            let image = non_empty(extract_col(line, col_image, Some(col_image_end)).trim());
            let status = non_empty(extract_col(line, col_status, Some(col_ports.unwrap_or(col_names))).trim());
            let ports = col_ports
                .map(|cp| extract_col(line, cp, Some(col_names)).trim().to_string())
                .and_then(|s| non_empty(&s));

            Some(container_entity(&name, container_id, image, status, ports))
        })
        .collect()
}

// --- docker images ---

/// Parse `docker images` tabular output.
///
/// Columns: REPOSITORY  TAG  IMAGE ID  CREATED  SIZE
fn parse_docker_images(stdout: &str) -> Vec<NewEntity> {
    let mut lines = stdout.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let col_repository = find_col_start(header, "REPOSITORY");
    let col_tag = find_col_start(header, "TAG");
    let col_image_id = find_col_start(header, "IMAGE ID");
    let col_created = find_col_start(header, "CREATED");

    let (col_repository, col_tag, col_image_id, col_created) =
        match (col_repository, col_tag, col_image_id, col_created) {
            (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
            _ => return Vec::new(),
        };

    lines
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let repo = extract_col(line, col_repository, Some(col_tag)).trim().to_string();
            let tag = extract_col(line, col_tag, Some(col_image_id)).trim().to_string();

            // Skip untagged / dangling images — no meaningful identity.
            if repo == "<none>" && tag == "<none>" {
                return None;
            }

            let image_id = non_empty(extract_col(line, col_image_id, Some(col_created)).trim());

            let full_name = if tag.is_empty() || tag == "<none>" {
                repo.clone()
            } else {
                format!("{repo}:{tag}")
            };

            Some(image_entity(&full_name, &repo, non_empty(&tag), image_id, None))
        })
        .collect()
}

// --- docker build ---

/// Parse `docker build` output for "Successfully built <id>" and
/// "Successfully tagged <name>:<tag>" lines.
fn parse_docker_build(stdout: &str) -> Vec<NewEntity> {
    let mut built_id: Option<String> = None;
    let mut tagged: Vec<String> = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Successfully built ") {
            built_id = non_empty(rest.trim());
        } else if let Some(rest) = trimmed.strip_prefix("Successfully tagged ") {
            if let Some(tag) = non_empty(rest.trim()) {
                tagged.push(tag);
            }
        }
    }

    if tagged.is_empty() {
        // No tagged images — emit one entity keyed on the build ID if present.
        built_id
            .map(|id| {
                vec![image_entity(
                    &id,
                    "",
                    None,
                    Some(id.clone()),
                    None,
                )]
            })
            .unwrap_or_default()
    } else {
        // One entity per tag, attaching the build ID as image_id.
        tagged
            .into_iter()
            .map(|full_tag| {
                let (repo, tag) = split_image_tag(&full_tag);
                image_entity(
                    &full_tag,
                    repo,
                    non_empty(tag),
                    built_id.clone(),
                    None,
                )
            })
            .collect()
    }
}

// --- docker-compose ps ---

/// Parse `docker-compose ps` output.
///
/// Classic compose ps format:
///   Name              Command    State    Ports
///   -----------------------------------------------
///   app_web_1   nginx ...   Up   0.0.0.0:80->80/tcp
///
/// New `docker compose ps` format is similar to `docker ps` — we fall through to
/// the docker ps parser in that case because the header contains "CONTAINER ID".
fn parse_compose_ps(stdout: &str) -> Vec<NewEntity> {
    let mut lines = stdout.lines().peekable();

    let header = match lines.next() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // New compose format (docker compose ps) has "CONTAINER ID" in header.
    if header.contains("CONTAINER ID") {
        let rebuilt = std::iter::once(header)
            .chain(lines)
            .collect::<Vec<_>>()
            .join("\n");
        return parse_docker_ps(&rebuilt);
    }

    // Classic compose ps: skip separator lines, parse NAME + COMMAND + STATE + PORTS.
    let col_name = find_col_start(header, "Name").or_else(|| find_col_start(header, "NAME"));
    let col_command =
        find_col_start(header, "Command").or_else(|| find_col_start(header, "COMMAND"));
    let col_state =
        find_col_start(header, "State").or_else(|| find_col_start(header, "STATE"));
    let col_ports =
        find_col_start(header, "Ports").or_else(|| find_col_start(header, "PORTS"));

    let (col_name, col_command, col_state) = match (col_name, col_command, col_state) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => return Vec::new(),
    };

    lines
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('-')
        })
        .filter_map(|line| {
            let name = extract_col(line, col_name, Some(col_command)).trim().to_string();
            if name.is_empty() {
                return None;
            }
            let state =
                non_empty(extract_col(line, col_state, col_ports).trim());
            let ports = col_ports
                .map(|cp| extract_col(line, cp, None).trim().to_string())
                .and_then(|s| non_empty(&s));

            Some(service_entity(&name, state, ports))
        })
        .collect()
}

// --- Entity constructors ---

fn container_entity(
    name: &str,
    container_id: Option<String>,
    image: Option<String>,
    status: Option<String>,
    ports: Option<String>,
) -> NewEntity {
    NewEntity {
        entity_type: "docker_container".into(),
        name: name.to_string(),
        canonical_key: name.to_string(),
        properties: None,
        typed_data: Some(TypedEntityData::DockerContainer {
            container_id,
            name: name.to_string(),
            image,
            status,
            ports,
        }),
        observation_context: None,
    }
}

fn image_entity(
    full_name: &str,
    repository: &str,
    tag: Option<String>,
    image_id: Option<String>,
    size_bytes: Option<i64>,
) -> NewEntity {
    NewEntity {
        entity_type: "docker_image".into(),
        name: full_name.to_string(),
        canonical_key: full_name.to_string(),
        properties: None,
        typed_data: Some(TypedEntityData::DockerImage {
            repository: repository.to_string(),
            tag,
            image_id,
            size_bytes,
        }),
        observation_context: None,
    }
}

fn service_entity(name: &str, _state: Option<String>, ports: Option<String>) -> NewEntity {
    NewEntity {
        entity_type: "docker_service".into(),
        name: name.to_string(),
        canonical_key: name.to_string(),
        properties: None,
        typed_data: Some(TypedEntityData::DockerService {
            name: name.to_string(),
            image: None,
            compose_file: None,
            ports,
        }),
        observation_context: None,
    }
}

// --- Column-extraction helpers ---

/// Find the byte-offset of `col_name` within `header`.
///
/// Returns the char-index of the first character of `col_name` in `header`,
/// or `None` if not found.
fn find_col_start(header: &str, col_name: &str) -> Option<usize> {
    header.find(col_name)
}

/// Extract the slice of `line` from `start` up to (but not including) `end`.
///
/// If the line is shorter than `start`, returns an empty string.
/// If `end` is None or the line is shorter than `end`, returns from `start` to EOL.
fn extract_col(line: &str, start: usize, end: Option<usize>) -> &str {
    if start >= line.len() {
        return "";
    }
    match end {
        Some(e) if e <= line.len() => line[start..e].trim_end(),
        _ => line[start..].trim_end(),
    }
}

/// Return `Some(s.to_string())` if `s` is non-empty, else `None`.
fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Split "repository:tag" into ("repository", "tag").
/// If no colon is present, returns (full_name, "").
fn split_image_tag(full: &str) -> (&str, &str) {
    match full.rfind(':') {
        Some(pos) => (&full[..pos], &full[pos + 1..]),
        None => (full, ""),
    }
}

/// Strip ANSI escape sequences (ESC+[...m color codes, cursor movement, OSC sequences).
/// Mirrors the implementation in `src/extract/git/mod.rs` — kept as a module-local
/// copy because `git::strip_ansi` is `pub(super)` (private to the `extract` module tree).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_color_codes() {
        let input = "\x1b[32mhello\x1b[0m world";
        assert_eq!(strip_ansi(input), "hello world");
    }

    #[test]
    fn extract_col_handles_short_line() {
        assert_eq!(extract_col("hi", 10, Some(20)), "");
    }

    #[test]
    fn split_image_tag_no_colon() {
        let (r, t) = split_image_tag("ubuntu");
        assert_eq!(r, "ubuntu");
        assert_eq!(t, "");
    }

    #[test]
    fn split_image_tag_with_tag() {
        let (r, t) = split_image_tag("nginx:latest");
        assert_eq!(r, "nginx");
        assert_eq!(t, "latest");
    }
}
