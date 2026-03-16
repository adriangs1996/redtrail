//! # ClaudeExecutor Parsing & Merging Tests
//!
//! Tests the parsing and KB-merging logic in ClaudeExecutor without spawning
//! any `claude -p` processes. These are the pure-function data transformation
//! tests that validate result extraction and knowledge base updates.

use redtrail::agent::claude_executor::{ClaudeExecutor, FindingReport, ParseOutcome, SpecialistResult};
use redtrail::agent::knowledge::{AccessLevel, HostInfo, KnowledgeBase};

// ===========================================================================
// parse_result_markers
// ===========================================================================

/// Valid JSON between markers should parse successfully.
#[test]
fn parse_markers_valid_json() {
    let output = r#"some preamble text
===REDTRAIL_RESULT===
{
  "discovered_hosts": [{"ip": "10.0.0.1", "ports": [22, 80], "services": ["ssh", "http"], "os": "Linux"}],
  "credentials": [],
  "flags": ["FLAG{test}"],
  "findings": [],
  "notes": ["found a host"]
}
===REDTRAIL_RESULT===
trailing text"#;

    let result = ClaudeExecutor::parse_result_markers(output);
    match result {
        ParseOutcome::Success(r) => {
            assert_eq!(r.discovered_hosts.len(), 1);
            assert_eq!(r.discovered_hosts[0].ip, "10.0.0.1");
            assert_eq!(r.flags, vec!["FLAG{test}"]);
            assert_eq!(r.notes, vec!["found a host"]);
        }
        other => panic!("Expected Success, got {other:?}"),
    }
}

/// No markers at all should return NoMarkers.
#[test]
fn parse_markers_no_markers() {
    let output = "just some regular output without any markers";
    match ClaudeExecutor::parse_result_markers(output) {
        ParseOutcome::NoMarkers => {} // expected
        other => panic!("Expected NoMarkers, got {other:?}"),
    }
}

/// Only one marker should return NoMarkers (need at least 2 to delimit JSON).
#[test]
fn parse_markers_single_marker() {
    let output = "text ===REDTRAIL_RESULT=== more text but no closing marker";
    match ClaudeExecutor::parse_result_markers(output) {
        ParseOutcome::NoMarkers => {}
        other => panic!("Expected NoMarkers, got {other:?}"),
    }
}

/// Invalid JSON between markers should return ParseError.
#[test]
fn parse_markers_invalid_json() {
    let output = "===REDTRAIL_RESULT===\n{this is not json}\n===REDTRAIL_RESULT===";
    match ClaudeExecutor::parse_result_markers(output) {
        ParseOutcome::ParseError(msg) => {
            assert!(!msg.is_empty(), "Error message should not be empty");
        }
        other => panic!("Expected ParseError, got {other:?}"),
    }
}

/// Empty JSON object between markers should parse (all fields have defaults).
#[test]
fn parse_markers_empty_json_object() {
    let output = "===REDTRAIL_RESULT===\n{}\n===REDTRAIL_RESULT===";
    match ClaudeExecutor::parse_result_markers(output) {
        ParseOutcome::Success(r) => {
            assert!(r.discovered_hosts.is_empty());
            assert!(r.credentials.is_empty());
            assert!(r.flags.is_empty());
            assert!(r.findings.is_empty());
            assert!(r.notes.is_empty());
        }
        other => panic!("Expected Success, got {other:?}"),
    }
}

/// Three markers — JSON should be between the first two only.
#[test]
fn parse_markers_multiple_markers() {
    let output = r#"===REDTRAIL_RESULT===
{"flags": ["FLAG{first}"]}
===REDTRAIL_RESULT===
===REDTRAIL_RESULT===
{"flags": ["FLAG{second}"]}
===REDTRAIL_RESULT==="#;

    match ClaudeExecutor::parse_result_markers(output) {
        ParseOutcome::Success(r) => {
            assert_eq!(r.flags, vec!["FLAG{first}"]);
        }
        other => panic!("Expected Success, got {other:?}"),
    }
}

/// Partial fields should parse — specialist only fills relevant fields.
#[test]
fn parse_markers_partial_fields() {
    let output = r#"===REDTRAIL_RESULT===
{"credentials": [{"username": "admin", "password": "pass123", "service": "ssh", "host": "10.0.0.1"}]}
===REDTRAIL_RESULT==="#;

    match ClaudeExecutor::parse_result_markers(output) {
        ParseOutcome::Success(r) => {
            assert_eq!(r.credentials.len(), 1);
            assert_eq!(r.credentials[0].username, "admin");
            assert!(r.discovered_hosts.is_empty());
        }
        other => panic!("Expected Success, got {other:?}"),
    }
}

// ===========================================================================
// merge_result_into_kb: host merging
// ===========================================================================

/// New hosts should be added to KB.
#[test]
fn merge_adds_new_hosts() {
    let mut kb = KnowledgeBase::new();
    let result = SpecialistResult {
        discovered_hosts: vec![
            HostInfo {
                ip: "10.0.0.1".into(),
                ports: vec![22, 80],
                services: vec!["ssh".into(), "http".into()],
                os: Some("Linux".into()),
            },
            HostInfo {
                ip: "10.0.0.2".into(),
                ports: vec![443],
                services: vec!["https".into()],
                os: None,
            },
        ],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.discovered_hosts.len(), 2);
    assert_eq!(kb.discovered_hosts[0].ip, "10.0.0.1");
    assert_eq!(kb.discovered_hosts[0].ports, vec![22, 80]);
    assert_eq!(kb.discovered_hosts[1].ip, "10.0.0.2");
}

/// Existing host should be updated with new ports/services, not duplicated.
#[test]
fn merge_deduplicates_hosts_and_adds_new_ports() {
    let mut kb = KnowledgeBase::new();
    kb.discovered_hosts.push(HostInfo {
        ip: "10.0.0.1".into(),
        ports: vec![22],
        services: vec!["ssh".into()],
        os: None,
    });

    let result = SpecialistResult {
        discovered_hosts: vec![HostInfo {
            ip: "10.0.0.1".into(),
            ports: vec![22, 80],
            services: vec!["ssh".into(), "http".into()],
            os: Some("Linux".into()),
        }],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.discovered_hosts.len(), 1, "Should not duplicate host");
    assert_eq!(kb.discovered_hosts[0].ports, vec![22, 80]);
    assert_eq!(
        kb.discovered_hosts[0].services,
        vec!["ssh".to_string(), "http".to_string()]
    );
    assert_eq!(kb.discovered_hosts[0].os, Some("Linux".into()));
}

/// OS should NOT be overwritten if already set.
#[test]
fn merge_does_not_overwrite_existing_os() {
    let mut kb = KnowledgeBase::new();
    kb.discovered_hosts.push(HostInfo {
        ip: "10.0.0.1".into(),
        ports: vec![22],
        services: vec!["ssh".into()],
        os: Some("Debian".into()),
    });

    let result = SpecialistResult {
        discovered_hosts: vec![HostInfo {
            ip: "10.0.0.1".into(),
            ports: vec![],
            services: vec![],
            os: Some("Ubuntu".into()),
        }],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(
        kb.discovered_hosts[0].os,
        Some("Debian".into()),
        "Should not overwrite existing OS"
    );
}

// ===========================================================================
// merge_result_into_kb: credential merging
// ===========================================================================

/// New credentials should be added.
#[test]
fn merge_adds_new_credentials() {
    let mut kb = KnowledgeBase::new();
    let result = SpecialistResult {
        credentials: vec![redtrail::agent::claude_executor::CredentialResult {
            username: "admin".into(),
            password: Some("pass123".into()),
            hash: None,
            service: "ssh".into(),
            host: "10.0.0.1".into(),
        }],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.credentials.len(), 1);
    assert_eq!(kb.credentials[0].username, "admin");
    assert_eq!(kb.credentials[0].password, Some("pass123".into()));
}

/// Duplicate credentials should not be added.
#[test]
fn merge_deduplicates_credentials() {
    let mut kb = KnowledgeBase::new();
    kb.credentials.push(redtrail::Credential {
        username: "admin".into(),
        password: Some("pass123".into()),
        hash: None,
        service: "ssh".into(),
        host: "10.0.0.1".into(),
    });

    let result = SpecialistResult {
        credentials: vec![redtrail::agent::claude_executor::CredentialResult {
            username: "admin".into(),
            password: Some("pass123".into()),
            hash: None,
            service: "ssh".into(),
            host: "10.0.0.1".into(),
        }],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.credentials.len(), 1, "Should not duplicate credential");
}

// ===========================================================================
// merge_result_into_kb: flag merging
// ===========================================================================

/// New flags should be added.
#[test]
fn merge_adds_new_flags() {
    let mut kb = KnowledgeBase::new();
    let result = SpecialistResult {
        flags: vec!["FLAG{alpha}".into(), "FLAG{beta}".into()],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.flags.len(), 2);
}

/// Duplicate flags should not be added.
#[test]
fn merge_deduplicates_flags() {
    let mut kb = KnowledgeBase::new();
    kb.flags.push("FLAG{alpha}".into());

    let result = SpecialistResult {
        flags: vec!["FLAG{alpha}".into(), "FLAG{beta}".into()],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.flags.len(), 2, "Should dedup FLAG{{alpha}}");
    assert!(kb.flags.contains(&"FLAG{alpha}".to_string()));
    assert!(kb.flags.contains(&"FLAG{beta}".to_string()));
}

// ===========================================================================
// merge_result_into_kb: access levels
// ===========================================================================

/// Access levels should be merged.
#[test]
fn merge_adds_access_levels() {
    let mut kb = KnowledgeBase::new();
    let result = SpecialistResult {
        access_levels: vec![AccessLevel {
            host: "10.0.0.1".into(),
            user: "root".into(),
            privilege_level: "critical".into(),
            method: "ssh".into(),
        }],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.access_levels.len(), 1);
    assert_eq!(kb.access_levels[0].user, "root");
}

// ===========================================================================
// convert_findings
// ===========================================================================

/// FindingReport should convert to Finding correctly.
#[test]
fn convert_findings_maps_fields() {
    let result = SpecialistResult {
        findings: vec![FindingReport {
            vuln_type: "SQL Injection".into(),
            severity: "Critical".into(),
            endpoint: "/login".into(),
            description: "SQL injection in login form".into(),
            evidence: "error in query".into(),
        }],
        ..Default::default()
    };

    let findings = ClaudeExecutor::convert_findings(&result);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].endpoint, "/login");
    assert_eq!(findings[0].description, "SQL injection in login form");
}

/// Empty findings should produce empty vec.
#[test]
fn convert_findings_empty() {
    let result = SpecialistResult::default();
    let findings = ClaudeExecutor::convert_findings(&result);
    assert!(findings.is_empty());
}

// ===========================================================================
// merge_result_into_kb: notes merging
// ===========================================================================

/// Notes should be appended to KB.
#[test]
fn merge_adds_notes() {
    let mut kb = KnowledgeBase::new();
    kb.notes.push("existing note".into());

    let result = SpecialistResult {
        notes: vec!["new note 1".into(), "new note 2".into()],
        ..Default::default()
    };

    ClaudeExecutor::merge_result_into_kb(&result, &mut kb);

    assert_eq!(kb.notes.len(), 3);
    assert_eq!(kb.notes[0], "existing note");
    assert_eq!(kb.notes[1], "new note 1");
}
