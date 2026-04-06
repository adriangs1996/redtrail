use crate::config::LlmConfig;
use crate::core::db::CommandRow;
use crate::core::secrets::engine::redact_secrets;
use crate::extract::types::{Extraction, NewEntity};
use std::collections::HashSet;
use std::time::Duration;

const ALLOWED_ENTITY_TYPES: &[&str] = &[
    "file",
    "ip_address",
    "url",
    "port",
    "error",
    "service",
    "package",
    "environment_variable",
];

pub struct LlmExtractor {
    config: LlmConfig,
}

impl LlmExtractor {
    /// Returns `None` if LLM extraction is disabled in config.
    pub fn new(config: &LlmConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        Some(Self {
            config: config.clone(),
        })
    }

    /// Attempt LLM-based entity extraction. Never returns an error — degrades
    /// to `Extraction::empty()` on any failure (network, parse, timeout).
    pub fn extract(&self, cmd: &CommandRow, stdout: &str) -> Extraction {
        let prompt = build_prompt(cmd, stdout, self.config.max_input_chars);

        let raw_response = match call_ollama(
            &self.config.ollama.url,
            &self.config.ollama.model,
            &prompt,
            self.config.timeout_seconds,
        ) {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("[redtrail] llm: ollama call failed: {e}");
                return Extraction::empty();
            }
        };

        let entities = parse_response(&raw_response);
        to_extraction(entities)
    }
}

fn build_prompt(cmd: &CommandRow, stdout: &str, max_chars: usize) -> String {
    let truncated = if stdout.len() > max_chars {
        &stdout[..max_chars]
    } else {
        stdout
    };

    let command_redacted = redact_secrets(&cmd.command_raw);
    let stdout_redacted = redact_secrets(truncated);
    let cwd = cmd.cwd.as_deref().unwrap_or("unknown");
    let exit_code = cmd.exit_code.map_or("unknown".to_string(), |c| c.to_string());

    format!(
        r#"You are an entity extraction system for terminal command output.
Extract structured entities from the following command and its output.

Command: {command_redacted}
Working directory: {cwd}
Exit code: {exit_code}

Output (truncated):
{stdout_redacted}

Extract entities as a JSON array. Each entity has:
- "entity_type": one of "file", "ip_address", "url", "port", "error", "service", "package", "environment_variable"
- "name": the entity value (e.g., file path, IP address, error message summary)
- "context": optional brief note on how this entity appeared

Return ONLY a JSON array, no other text. If no entities found, return []."#
    )
}

fn call_ollama(
    url: &str,
    model: &str,
    prompt: &str,
    timeout_secs: u64,
) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build()
        .into();

    let endpoint = format!("{url}/api/generate");
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false
    });

    let mut response = agent
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .send(body.to_string().as_bytes())
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let response_text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed to read response body: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&response_text).map_err(|e| format!("invalid JSON response: {e}"))?;

    json.get("response")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "missing 'response' field in Ollama output".to_string())
}

#[derive(serde::Deserialize)]
struct LlmEntity {
    entity_type: String,
    name: String,
    #[serde(default)]
    context: Option<String>,
}

fn parse_response(raw: &str) -> Vec<LlmEntity> {
    // Try direct parse first.
    if let Ok(entities) = serde_json::from_str::<Vec<LlmEntity>>(raw) {
        return entities;
    }

    // Try extracting JSON array from surrounding text.
    if let Some(extracted) = extract_json_array(raw)
        && let Ok(entities) = serde_json::from_str::<Vec<LlmEntity>>(&extracted)
    {
        return entities;
    }

    // Try stripping markdown code fences.
    let stripped = strip_code_fences(raw);
    if stripped != raw {
        if let Ok(entities) = serde_json::from_str::<Vec<LlmEntity>>(&stripped) {
            return entities;
        }
        if let Some(extracted) = extract_json_array(&stripped)
            && let Ok(entities) = serde_json::from_str::<Vec<LlmEntity>>(&extracted)
        {
            return entities;
        }
    }

    Vec::new()
}

fn extract_json_array(text: &str) -> Option<String> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if start < end {
        Some(text[start..=end].to_string())
    } else {
        None
    }
}

fn strip_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```json")
        && let Some(inner) = rest.strip_suffix("```")
    {
        return inner.trim().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("```")
        && let Some(inner) = rest.strip_suffix("```")
    {
        return inner.trim().to_string();
    }
    trimmed.to_string()
}

fn to_extraction(entities: Vec<LlmEntity>) -> Extraction {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for entity in entities {
        if !ALLOWED_ENTITY_TYPES.contains(&entity.entity_type.as_str()) {
            continue;
        }
        if entity.name.trim().is_empty() {
            continue;
        }

        let canonical_key = format!("{}:{}", entity.entity_type, entity.name);
        if !seen.insert(canonical_key.clone()) {
            continue;
        }

        result.push(NewEntity {
            entity_type: entity.entity_type,
            name: entity.name,
            canonical_key,
            properties: entity
                .context
                .as_ref()
                .map(|c| serde_json::json!({ "context": c })),
            typed_data: None,
            observation_context: Some("llm-extracted".to_string()),
        });
    }

    Extraction {
        entities: result,
        relationships: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cmd(command_raw: &str, cwd: &str, exit_code: Option<i32>) -> CommandRow {
        CommandRow {
            id: "test-id".to_string(),
            session_id: "test-session".to_string(),
            command_raw: command_raw.to_string(),
            command_binary: None,
            cwd: Some(cwd.to_string()),
            exit_code,
            hostname: None,
            shell: None,
            source: "human".to_string(),
            timestamp_start: 0,
            timestamp_end: None,
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            redacted: false,
            tool_name: None,
            command_subcommand: None,
            git_repo: None,
            git_branch: None,
            agent_session_id: None,
        }
    }

    #[test]
    fn test_build_prompt_truncates_stdout() {
        let cmd = make_cmd("npm run build", "/app", Some(1));
        let long_output = "MARKER_A".to_string() + &"z".repeat(10000) + "MARKER_B";
        let prompt = build_prompt(&cmd, &long_output, 100);
        // MARKER_A is within the first 100 chars so it should appear
        assert!(prompt.contains("MARKER_A"), "start of stdout should be in prompt");
        // MARKER_B is at the end (>10000 chars in) so it should be truncated
        assert!(!prompt.contains("MARKER_B"), "end of stdout should be truncated");
    }

    #[test]
    fn test_build_prompt_redacts_secrets() {
        let cmd = make_cmd(
            "curl -H 'Authorization: Bearer AKIAIOSFODNN7EXAMPLE'",
            "/app",
            Some(0),
        );
        let stdout = "response ok";
        let prompt = build_prompt(&cmd, stdout, 4096);
        assert!(
            !prompt.contains("AKIAIOSFODNN7EXAMPLE"),
            "secrets must be redacted from prompt"
        );
    }

    #[test]
    fn test_parse_response_valid_json() {
        let raw = r#"[{"entity_type":"file","name":"/src/main.rs"}]"#;
        let entities = parse_response(raw);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "file");
        assert_eq!(entities[0].name, "/src/main.rs");
    }

    #[test]
    fn test_parse_response_json_in_markdown_fence() {
        let raw = "```json\n[{\"entity_type\":\"url\",\"name\":\"https://example.com\"}]\n```";
        let entities = parse_response(raw);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "url");
    }

    #[test]
    fn test_parse_response_with_preamble() {
        let raw = "Here are the entities I found:\n[{\"entity_type\":\"port\",\"name\":\"8080\"}]";
        let entities = parse_response(raw);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "8080");
    }

    #[test]
    fn test_parse_response_malformed() {
        let raw = "this is not json at all, no brackets here";
        let entities = parse_response(raw);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_parse_response_empty_array() {
        let raw = "[]";
        let entities = parse_response(raw);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_to_extraction_filters_unknown_types() {
        let entities = vec![
            LlmEntity {
                entity_type: "file".to_string(),
                name: "/valid".to_string(),
                context: None,
            },
            LlmEntity {
                entity_type: "banana".to_string(),
                name: "invalid".to_string(),
                context: None,
            },
        ];
        let extraction = to_extraction(entities);
        assert_eq!(extraction.entities.len(), 1);
        assert_eq!(extraction.entities[0].entity_type, "file");
    }

    #[test]
    fn test_to_extraction_deduplicates() {
        let entities = vec![
            LlmEntity {
                entity_type: "file".to_string(),
                name: "/app/main.rs".to_string(),
                context: Some("first mention".to_string()),
            },
            LlmEntity {
                entity_type: "file".to_string(),
                name: "/app/main.rs".to_string(),
                context: Some("second mention".to_string()),
            },
        ];
        let extraction = to_extraction(entities);
        assert_eq!(extraction.entities.len(), 1);
    }

    #[test]
    fn test_to_extraction_sets_observation_context() {
        let entities = vec![LlmEntity {
            entity_type: "service".to_string(),
            name: "nginx".to_string(),
            context: None,
        }];
        let extraction = to_extraction(entities);
        assert_eq!(
            extraction.entities[0].observation_context,
            Some("llm-extracted".to_string())
        );
    }

    #[test]
    fn test_to_extraction_skips_empty_names() {
        let entities = vec![LlmEntity {
            entity_type: "file".to_string(),
            name: "   ".to_string(),
            context: None,
        }];
        let extraction = to_extraction(entities);
        assert!(extraction.is_empty());
    }

    #[test]
    fn test_extract_json_array_with_surrounding_text() {
        let text = "Some text before [{\"a\":1}] and after";
        let result = extract_json_array(text);
        assert_eq!(result, Some("[{\"a\":1}]".to_string()));
    }

    #[test]
    fn test_strip_code_fences_json() {
        let input = "```json\n[1,2,3]\n```";
        assert_eq!(strip_code_fences(input), "[1,2,3]");
    }

    #[test]
    fn test_strip_code_fences_plain() {
        let input = "```\n[1,2,3]\n```";
        assert_eq!(strip_code_fences(input), "[1,2,3]");
    }

    #[test]
    fn test_strip_code_fences_no_fences() {
        let input = "[1,2,3]";
        assert_eq!(strip_code_fences(input), "[1,2,3]");
    }
}
