use colored::Colorize;
use regex::Regex;

use crate::types::{Evidence, Finding, HttpRequest, HttpResponse, Severity, VulnType};

/// Detects flag patterns in text, deduplicates them, and auto-creates findings.
pub struct FlagDetector {
    seen: Vec<String>,
    pattern: Regex,
}

impl Default for FlagDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl FlagDetector {
    pub fn new() -> Self {
        Self {
            seen: Vec::new(),
            pattern: Regex::new(r"FLAG\{[^}]+\}").expect("invalid FLAG regex"),
        }
    }

    /// Create a FlagDetector with a custom regex pattern.
    pub fn with_pattern(pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            seen: Vec::new(),
            pattern: Regex::new(pattern)?,
        })
    }

    /// Scan text for FLAG{...} patterns. Returns newly discovered flags (not seen before).
    /// For each new flag: logs to console, adds to knowledge base flags, and creates a Finding.
    pub fn scan(
        &mut self,
        text: &str,
        kb_flags: &mut Vec<String>,
        findings: &mut Vec<Finding>,
    ) -> Vec<String> {
        let mut new_flags = Vec::new();

        for m in self.pattern.find_iter(text) {
            let flag = m.as_str().to_string();
            if self.seen.contains(&flag) {
                continue;
            }
            self.seen.push(flag.clone());

            // Add to knowledge base flags (deduplicated)
            if !kb_flags.contains(&flag) {
                kb_flags.push(flag.clone());
            }

            // Log to console
            println!(
                "{} Captured flag: {}",
                "[FLAG]".green().bold(),
                flag.yellow().bold()
            );

            // Auto-create a Finding
            findings.push(Finding {
                vuln_type: VulnType::InformationDisclosure,
                severity: Severity::Info,
                endpoint: "flag-capture".to_string(),
                evidence: vec![Evidence {
                    request: HttpRequest {
                        method: String::new(),
                        url: String::new(),
                        headers: std::collections::HashMap::new(),
                        body: None,
                    },
                    response: HttpResponse {
                        status_code: 0,
                        headers: std::collections::HashMap::new(),
                        body: String::new(),
                        elapsed_ms: 0,
                    },
                    payload: flag.clone(),
                    description: format!("Flag captured: {flag}"),
                }],
                description: format!("Captured flag: {flag}"),
                fix_suggestion: "Flags should not be exposed in application output".to_string(),
            });

            new_flags.push(flag);
        }

        new_flags
    }

    /// Number of unique flags detected so far.
    pub fn count(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_detects_single_flag() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        let new = detector.scan(
            "output contains FLAG{secret_123}",
            &mut kb_flags,
            &mut findings,
        );

        assert_eq!(new, vec!["FLAG{secret_123}"]);
        assert_eq!(kb_flags, vec!["FLAG{secret_123}"]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].description, "Captured flag: FLAG{secret_123}");
    }

    #[test]
    fn test_scan_detects_multiple_flags() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        let new = detector.scan("FLAG{first} and FLAG{second}", &mut kb_flags, &mut findings);

        assert_eq!(new.len(), 2);
        assert!(new.contains(&"FLAG{first}".to_string()));
        assert!(new.contains(&"FLAG{second}".to_string()));
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn test_scan_deduplicates() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        detector.scan("FLAG{dup}", &mut kb_flags, &mut findings);
        let new = detector.scan("FLAG{dup}", &mut kb_flags, &mut findings);

        assert!(new.is_empty());
        assert_eq!(kb_flags.len(), 1);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_scan_no_flags() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        let new = detector.scan("no flags here", &mut kb_flags, &mut findings);

        assert!(new.is_empty());
        assert!(kb_flags.is_empty());
        assert!(findings.is_empty());
    }

    #[test]
    fn test_scan_preserves_existing_kb_flags() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = vec!["FLAG{existing}".to_string()];
        let mut findings = Vec::new();

        detector.scan("FLAG{new_one}", &mut kb_flags, &mut findings);

        assert_eq!(kb_flags.len(), 2);
        assert!(kb_flags.contains(&"FLAG{existing}".to_string()));
        assert!(kb_flags.contains(&"FLAG{new_one}".to_string()));
    }

    #[test]
    fn test_scan_kb_deduplication() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = vec!["FLAG{already}".to_string()];
        let mut findings = Vec::new();

        // Flag already in kb_flags but not seen by detector
        detector.scan("FLAG{already}", &mut kb_flags, &mut findings);

        // Should not duplicate in kb_flags
        assert_eq!(kb_flags.len(), 1);
        // But still creates a finding (detector hadn't seen it)
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_count() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        assert_eq!(detector.count(), 0);

        detector.scan("FLAG{a} FLAG{b}", &mut kb_flags, &mut findings);
        assert_eq!(detector.count(), 2);

        detector.scan("FLAG{a}", &mut kb_flags, &mut findings);
        assert_eq!(detector.count(), 2); // no change
    }

    #[test]
    fn test_regex_edge_cases() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        // Nested braces should not match
        let new = detector.scan("FLAG{}", &mut kb_flags, &mut findings);
        assert!(new.is_empty());

        // Flag with special chars
        let new = detector.scan("FLAG{h3ll0_w0rld!}", &mut kb_flags, &mut findings);
        assert_eq!(new.len(), 1);

        // Flag embedded in JSON
        let new = detector.scan(
            r#"{"result": "FLAG{json_flag}", "status": "ok"}"#,
            &mut kb_flags,
            &mut findings,
        );
        assert_eq!(new, vec!["FLAG{json_flag}"]);
    }

    #[test]
    fn test_finding_has_correct_structure() {
        let mut detector = FlagDetector::new();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        detector.scan("FLAG{test_struct}", &mut kb_flags, &mut findings);

        let f = &findings[0];
        assert_eq!(f.vuln_type, VulnType::InformationDisclosure);
        assert_eq!(f.severity, Severity::Info);
        assert_eq!(f.endpoint, "flag-capture");
        assert_eq!(f.evidence.len(), 1);
        assert_eq!(f.evidence[0].payload, "FLAG{test_struct}");
    }

    #[test]
    fn test_custom_pattern() {
        let mut detector = FlagDetector::with_pattern(r"HTB\{[^}]+\}").unwrap();
        let mut kb_flags = Vec::new();
        let mut findings = Vec::new();

        // Should match HTB{...} pattern
        let new = detector.scan("found HTB{h4ck_th3_b0x}", &mut kb_flags, &mut findings);
        assert_eq!(new, vec!["HTB{h4ck_th3_b0x}"]);
        assert_eq!(findings.len(), 1);

        // Should NOT match default FLAG{...} pattern
        let new = detector.scan("FLAG{not_matched}", &mut kb_flags, &mut findings);
        assert!(new.is_empty());
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_with_pattern_invalid_regex() {
        let result = FlagDetector::with_pattern(r"[invalid");
        assert!(result.is_err());
    }
}
