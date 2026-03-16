use serde::{Deserialize, Serialize};

use crate::agent::knowledge::{AccessLevel, HostInfo, KnowledgeBase};
use crate::types::{Credential, Finding};

/// Unified result type that all specialists produce in orchestrator mode.
/// All fields use `#[serde(default)]` so specialists only need to fill
/// the fields relevant to their domain.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpecialistResult {
    #[serde(default)]
    pub discovered_hosts: Vec<HostInfo>,
    #[serde(default)]
    pub credentials: Vec<CredentialResult>,
    #[serde(default)]
    pub access_levels: Vec<AccessLevel>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub findings: Vec<FindingReport>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Credential as reported by a specialist (simplified for JSON output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialResult {
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub host: String,
}

/// A vulnerability finding reported by a specialist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingReport {
    pub vuln_type: String,
    pub severity: String,
    pub endpoint: String,
    pub description: String,
    #[serde(default)]
    pub evidence: String,
}

/// The result of parsing Claude Code's output for redtrail markers.
#[derive(Debug)]
pub enum ParseOutcome {
    Success(SpecialistResult),
    ParseError(String),
    NoMarkers,
}

/// Executor for LLM interactions. Simplified from the original autonomous executor —
/// keeps result types and parsing, removes specialist dispatch and process spawning.
#[derive(Clone)]
pub struct ClaudeExecutor {
    pub timeout_secs: u64,
    pub verbose: bool,
}

impl Default for ClaudeExecutor {
    fn default() -> Self {
        Self {
            timeout_secs: 1800,
            verbose: false,
        }
    }
}

impl ClaudeExecutor {
    pub fn new(timeout_secs: u64, verbose: bool) -> Self {
        Self {
            timeout_secs,
            verbose,
        }
    }

    /// Parse the output for `===REDTRAIL_RESULT===` markers and extract JSON.
    pub fn parse_result_markers(output: &str) -> ParseOutcome {
        let marker = "===REDTRAIL_RESULT===";
        let parts: Vec<&str> = output.split(marker).collect();

        if parts.len() < 3 {
            return ParseOutcome::NoMarkers;
        }

        let json_str = parts[1].trim();

        match serde_json::from_str::<SpecialistResult>(json_str) {
            Ok(result) => ParseOutcome::Success(result),
            Err(e) => ParseOutcome::ParseError(format!("{}: {}", e, json_str)),
        }
    }

    /// Convert a SpecialistResult into KB updates and findings.
    pub fn merge_result_into_kb(result: &SpecialistResult, kb: &mut KnowledgeBase) {
        // Merge discovered hosts
        for host in &result.discovered_hosts {
            if let Some(existing) = kb.discovered_hosts.iter_mut().find(|h| h.ip == host.ip) {
                for port in &host.ports {
                    if !existing.ports.contains(port) {
                        existing.ports.push(*port);
                    }
                }
                for service in &host.services {
                    if !existing.services.contains(service) {
                        existing.services.push(service.clone());
                    }
                }
                if existing.os.is_none() && host.os.is_some() {
                    existing.os = host.os.clone();
                }
            } else {
                kb.discovered_hosts.push(host.clone());
            }
        }

        // Merge credentials
        for cred in &result.credentials {
            let exists = kb
                .credentials
                .iter()
                .any(|c| c.username == cred.username && c.password == cred.password);
            if !exists {
                kb.credentials.push(Credential {
                    username: cred.username.clone(),
                    password: cred.password.clone(),
                    hash: cred.hash.clone(),
                    service: cred.service.clone(),
                    host: cred.host.clone(),
                });
            }
        }

        // Merge access levels
        for level in &result.access_levels {
            if !kb.access_levels.contains(level) {
                kb.access_levels.push(level.clone());
            }
        }

        // Merge flags
        for flag in &result.flags {
            if !kb.flags.contains(flag) {
                kb.flags.push(flag.clone());
            }
        }

        // Merge notes
        for note in &result.notes {
            if !kb.notes.contains(note) {
                kb.notes.push(note.clone());
            }
        }
    }

    /// Convert FindingReports from a SpecialistResult into proper Findings.
    pub fn convert_findings(result: &SpecialistResult) -> Vec<Finding> {
        result
            .findings
            .iter()
            .map(|f| {
                let vuln_type = parse_vuln_type(&f.vuln_type);
                let severity = parse_severity(&f.severity);
                Finding {
                    vuln_type,
                    severity,
                    endpoint: f.endpoint.clone(),
                    evidence: vec![],
                    description: f.description.clone(),
                    fix_suggestion: String::new(),
                }
            })
            .collect()
    }
}

fn parse_vuln_type(s: &str) -> crate::types::VulnType {
    use crate::types::VulnType;
    match s.to_lowercase().as_str() {
        "sqli" | "sqlinjection" | "sql_injection" => VulnType::SqlInjection,
        "xss" | "reflectedxss" | "reflected_xss" => VulnType::ReflectedXSS,
        "storedxss" | "stored_xss" => VulnType::StoredXSS,
        "idor" => VulnType::IDOR,
        "privesc" | "privilegeescalation" | "privilege_escalation" => VulnType::PrivilegeEscalation,
        "commandinjection" | "command_injection" | "rce" => VulnType::CommandInjection,
        "informationdisclosure" | "information_disclosure" | "infodisclosure" => {
            VulnType::InformationDisclosure
        }
        "weakcredentials" | "weak_credentials" => VulnType::WeakCredentials,
        "defaultcredentials" | "default_credentials" => VulnType::DefaultCredentials,
        "anonymousaccess" | "anonymous_access" => VulnType::AnonymousAccess,
        "missingauthentication" | "missing_authentication" | "noauth" => {
            VulnType::MissingAuthentication
        }
        "ssrf" => VulnType::SSRF,
        "csrf" => VulnType::CSRF,
        "ssti" => VulnType::SSTI,
        "fileupload" | "file_upload" => VulnType::FileUpload,
        "pathtraversal" | "path_traversal" | "lfi" | "rfi" => VulnType::PathTraversal,
        "bufferoverflow" | "buffer_overflow" => VulnType::BufferOverflow,
        "containerescape" | "container_escape" => VulnType::ContainerEscape,
        "sensitivedataexposure" | "sensitive_data_exposure" | "dataexposure" => {
            VulnType::SensitiveDataExposure
        }
        "insecureconfiguration" | "insecure_configuration" | "misconfig" | "misconfiguration" => {
            VulnType::InsecureConfiguration
        }
        _ => VulnType::InformationDisclosure,
    }
}

fn parse_severity(s: &str) -> crate::types::Severity {
    use crate::types::Severity;
    match s.to_lowercase().as_str() {
        "critical" => Severity::Critical,
        "high" => Severity::High,
        "medium" => Severity::Medium,
        "low" => Severity::Low,
        _ => Severity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_result_markers_success() {
        let output = r#"Some preamble text...

===REDTRAIL_RESULT===
{
  "discovered_hosts": [{"ip": "10.0.0.1", "ports": [80, 443], "services": ["http"], "os": "Linux"}],
  "flags": ["FLAG{test_flag}"],
  "notes": ["Found interesting service"]
}
===REDTRAIL_RESULT===

Some trailing text"#;

        match ClaudeExecutor::parse_result_markers(output) {
            ParseOutcome::Success(result) => {
                assert_eq!(result.discovered_hosts.len(), 1);
                assert_eq!(result.discovered_hosts[0].ip, "10.0.0.1");
                assert_eq!(result.discovered_hosts[0].ports, vec![80, 443]);
                assert_eq!(result.flags, vec!["FLAG{test_flag}"]);
                assert_eq!(result.notes, vec!["Found interesting service"]);
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_result_markers_no_markers() {
        let output = "Just some regular output without any markers";
        match ClaudeExecutor::parse_result_markers(output) {
            ParseOutcome::NoMarkers => {}
            other => panic!("Expected NoMarkers, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_result_markers_invalid_json() {
        let output = "===REDTRAIL_RESULT===\n{invalid json}\n===REDTRAIL_RESULT===";
        match ClaudeExecutor::parse_result_markers(output) {
            ParseOutcome::ParseError(_) => {}
            other => panic!("Expected ParseError, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_result_markers_partial_fields() {
        let output = r#"===REDTRAIL_RESULT===
{"flags": ["FLAG{only_flags}"], "notes": ["just notes"]}
===REDTRAIL_RESULT==="#;

        match ClaudeExecutor::parse_result_markers(output) {
            ParseOutcome::Success(result) => {
                assert!(result.discovered_hosts.is_empty());
                assert!(result.credentials.is_empty());
                assert_eq!(result.flags, vec!["FLAG{only_flags}"]);
                assert_eq!(result.notes, vec!["just notes"]);
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_result_markers_empty_json() {
        let output = "===REDTRAIL_RESULT===\n{}\n===REDTRAIL_RESULT===";
        match ClaudeExecutor::parse_result_markers(output) {
            ParseOutcome::Success(result) => {
                assert!(result.discovered_hosts.is_empty());
                assert!(result.credentials.is_empty());
                assert!(result.flags.is_empty());
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[test]
    fn test_specialist_result_deserialization_with_credentials() {
        let json = r#"{
            "credentials": [
                {"username": "admin", "password": "secret", "service": "ssh", "host": "10.0.0.1"}
            ],
            "findings": [
                {"vuln_type": "WeakCredentials", "severity": "High", "endpoint": "ssh://10.0.0.1",
                 "description": "Weak SSH credentials", "evidence": "hydra output"}
            ]
        }"#;

        let result: SpecialistResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.credentials.len(), 1);
        assert_eq!(result.credentials[0].username, "admin");
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].vuln_type, "WeakCredentials");
    }

    #[test]
    fn test_merge_result_into_kb() {
        let result = SpecialistResult {
            discovered_hosts: vec![HostInfo {
                ip: "10.0.0.1".to_string(),
                ports: vec![80, 443],
                services: vec!["http".to_string()],
                os: Some("Linux".to_string()),
            }],
            credentials: vec![CredentialResult {
                username: "admin".to_string(),
                password: Some("pass".to_string()),
                hash: None,
                service: "ssh".to_string(),
                host: "10.0.0.1".to_string(),
            }],
            flags: vec!["FLAG{test}".to_string()],
            notes: vec!["interesting".to_string()],
            ..Default::default()
        };

        let mut kb = KnowledgeBase::new();
        ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

        assert_eq!(kb.discovered_hosts.len(), 1);
        assert_eq!(kb.credentials.len(), 1);
        assert_eq!(kb.flags, vec!["FLAG{test}"]);
        assert_eq!(kb.notes, vec!["interesting"]);
    }

    #[test]
    fn test_merge_result_deduplicates() {
        let result = SpecialistResult {
            flags: vec!["FLAG{dup}".to_string()],
            ..Default::default()
        };

        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{dup}".to_string());

        ClaudeExecutor::merge_result_into_kb(&result, &mut kb);
        assert_eq!(kb.flags.len(), 1);
    }

    #[test]
    fn test_convert_findings() {
        let result = SpecialistResult {
            findings: vec![
                FindingReport {
                    vuln_type: "SQLi".to_string(),
                    severity: "Critical".to_string(),
                    endpoint: "/api/users".to_string(),
                    description: "SQL injection found".to_string(),
                    evidence: "payload worked".to_string(),
                },
                FindingReport {
                    vuln_type: "WeakCredentials".to_string(),
                    severity: "High".to_string(),
                    endpoint: "ssh://10.0.0.1".to_string(),
                    description: "Default creds".to_string(),
                    evidence: "admin:admin".to_string(),
                },
            ],
            ..Default::default()
        };

        let findings = ClaudeExecutor::convert_findings(&result);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].vuln_type, crate::types::VulnType::SqlInjection);
        assert_eq!(findings[0].severity, crate::types::Severity::Critical);
        assert_eq!(
            findings[1].vuln_type,
            crate::types::VulnType::WeakCredentials
        );
    }

    #[test]
    fn test_parse_vuln_type_variants() {
        assert_eq!(
            parse_vuln_type("sqli"),
            crate::types::VulnType::SqlInjection
        );
        assert_eq!(parse_vuln_type("XSS"), crate::types::VulnType::ReflectedXSS);
        assert_eq!(parse_vuln_type("SSRF"), crate::types::VulnType::SSRF);
        assert_eq!(
            parse_vuln_type("RCE"),
            crate::types::VulnType::CommandInjection
        );
        assert_eq!(
            parse_vuln_type("unknown_type"),
            crate::types::VulnType::InformationDisclosure
        );
    }

    #[test]
    fn test_parse_severity_variants() {
        assert_eq!(parse_severity("Critical"), crate::types::Severity::Critical);
        assert_eq!(parse_severity("HIGH"), crate::types::Severity::High);
        assert_eq!(parse_severity("medium"), crate::types::Severity::Medium);
        assert_eq!(parse_severity("Low"), crate::types::Severity::Low);
        assert_eq!(parse_severity("info"), crate::types::Severity::Info);
        assert_eq!(parse_severity("unknown"), crate::types::Severity::Info);
    }
}
