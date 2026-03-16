use std::fs;
use std::path::Path;

use tera::{Context, Tera};

use crate::agent::knowledge::KnowledgeBase;
#[cfg(test)]
use crate::types::ExecMode;
use crate::types::{Finding, Severity, Target, VulnType};

const TEMPLATE: &str = include_str!("templates/report.html");

pub fn fix_suggestion_for(vuln_type: &VulnType) -> &'static str {
    match vuln_type {
        VulnType::SqlInjection => "Use parameterized queries",
        VulnType::StoredXSS | VulnType::ReflectedXSS => {
            "Escape all user-controlled output. Remove unsafe template directives such as |safe. Consider adding Content-Security-Policy headers."
        }
        VulnType::IDOR | VulnType::PrivilegeEscalation => {
            "Implement authorization checks verifying resource ownership"
        }
        VulnType::CommandInjection => {
            "Sanitize inputs and avoid passing user data to shell commands"
        }
        VulnType::InformationDisclosure | VulnType::SensitiveDataExposure => {
            "Remove sensitive data from responses and disable verbose error messages"
        }
        VulnType::WeakCredentials | VulnType::DefaultCredentials => {
            "Enforce strong password policies and change default credentials"
        }
        VulnType::AnonymousAccess | VulnType::MissingAuthentication => {
            "Require authentication for all sensitive endpoints"
        }
        VulnType::SSRF => "Validate and restrict outbound requests to allowed destinations",
        VulnType::CSRF => "Implement anti-CSRF tokens on state-changing requests",
        VulnType::SSTI => "Avoid user-controlled template input; use sandboxed template engines",
        VulnType::FileUpload => {
            "Validate file types, restrict upload paths, and scan uploaded files"
        }
        VulnType::Deserialization => {
            "Avoid deserializing untrusted data; use safe serialization formats"
        }
        VulnType::PathTraversal => "Canonicalize paths and restrict access to allowed directories",
        VulnType::BufferOverflow | VulnType::FormatString => {
            "Use memory-safe languages or bounds-checked APIs"
        }
        VulnType::KerberosAttack | VulnType::PassTheHash | VulnType::GoldenTicket => {
            "Enforce strong Kerberos policies and rotate service account credentials"
        }
        VulnType::ContainerEscape
        | VulnType::DockerSocketExposure
        | VulnType::KubernetesExposure => {
            "Restrict container privileges and protect orchestration APIs"
        }
        VulnType::SupplyChainCompromise
        | VulnType::DependencyConfusion
        | VulnType::CiCdInjection => "Verify dependency integrity and secure CI/CD pipelines",
        VulnType::PromptInjection | VulnType::ToolPoisoning => {
            "Validate and sanitize AI/LLM inputs; enforce tool allow-lists"
        }
        VulnType::DNSZoneTransfer => "Restrict DNS zone transfers to authorized servers",
        VulnType::SMBMisconfiguration => {
            "Disable unnecessary SMB shares and enforce authentication"
        }
        VulnType::DataExfiltration => "Monitor and restrict outbound data flows",
        VulnType::InsecureConfiguration => {
            "Review and harden configuration against security benchmarks"
        }
    }
}

pub fn calculate_score(findings: &[Finding]) -> u32 {
    let deduction: u32 = findings
        .iter()
        .map(|f| match f.severity {
            Severity::Critical => 30,
            Severity::High => 20,
            Severity::Medium => 10,
            Severity::Low => 5,
            Severity::Info => 0,
        })
        .sum();

    100u32.saturating_sub(deduction)
}

pub fn score_color(score: u32) -> &'static str {
    match score {
        0..=30 => "#ff4444",
        31..=60 => "#ff8c00",
        61..=80 => "#ffd700",
        _ => "#4caf50",
    }
}

fn severity_class(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "critical",
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
        Severity::Info => "info",
    }
}

fn vuln_type_title(vuln_type: &VulnType) -> &'static str {
    match vuln_type {
        VulnType::SqlInjection => "SQL Injection",
        VulnType::StoredXSS => "Stored XSS",
        VulnType::ReflectedXSS => "Reflected XSS",
        VulnType::IDOR => "Insecure Direct Object Reference",
        VulnType::PrivilegeEscalation => "Privilege Escalation",
        VulnType::CommandInjection => "Command Injection",
        VulnType::InformationDisclosure => "Information Disclosure",
        VulnType::WeakCredentials => "Weak Credentials",
        VulnType::DefaultCredentials => "Default Credentials",
        VulnType::AnonymousAccess => "Anonymous Access",
        VulnType::MissingAuthentication => "Missing Authentication",
        VulnType::SSRF => "Server-Side Request Forgery",
        VulnType::CSRF => "Cross-Site Request Forgery",
        VulnType::SSTI => "Server-Side Template Injection",
        VulnType::FileUpload => "Unrestricted File Upload",
        VulnType::Deserialization => "Insecure Deserialization",
        VulnType::PathTraversal => "Path Traversal",
        VulnType::BufferOverflow => "Buffer Overflow",
        VulnType::FormatString => "Format String Vulnerability",
        VulnType::KerberosAttack => "Kerberos Attack",
        VulnType::ContainerEscape => "Container Escape",
        VulnType::DockerSocketExposure => "Docker Socket Exposure",
        VulnType::KubernetesExposure => "Kubernetes Exposure",
        VulnType::SupplyChainCompromise => "Supply Chain Compromise",
        VulnType::CiCdInjection => "CI/CD Injection",
        VulnType::PromptInjection => "Prompt Injection",
        VulnType::ToolPoisoning => "Tool Poisoning",
        VulnType::DNSZoneTransfer => "DNS Zone Transfer",
        VulnType::SMBMisconfiguration => "SMB Misconfiguration",
        VulnType::PassTheHash => "Pass-the-Hash",
        VulnType::GoldenTicket => "Golden Ticket Attack",
        VulnType::DependencyConfusion => "Dependency Confusion",
        VulnType::DataExfiltration => "Data Exfiltration",
        VulnType::InsecureConfiguration => "Insecure Configuration",
        VulnType::SensitiveDataExposure => "Sensitive Data Exposure",
    }
}

fn severity_label(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "Critical",
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
        Severity::Info => "Info",
    }
}

fn count_severity(findings: &[Finding], severity: &Severity) -> usize {
    findings.iter().filter(|f| &f.severity == severity).count()
}

pub fn generate_html(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
) -> Result<String, tera::Error> {
    generate_html_with_flags(findings, target, timeline, scan_date, 0)
}

pub fn generate_html_with_flags(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
    flag_count: usize,
) -> Result<String, tera::Error> {
    generate_html_with_knowledge(findings, target, timeline, scan_date, flag_count, None)
}

pub fn generate_html_with_knowledge(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
    flag_count: usize,
    knowledge: Option<&KnowledgeBase>,
) -> Result<String, tera::Error> {
    generate_html_with_knowledge_inner(
        findings, target, timeline, scan_date, flag_count, knowledge, None,
    )
}

fn generate_html_with_knowledge_inner(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
    flag_count: usize,
    knowledge: Option<&KnowledgeBase>,
    historical: Option<&HistoricalMetrics>,
) -> Result<String, tera::Error> {
    let mut tera = Tera::default();
    tera.autoescape_on(vec![]);
    tera.add_raw_template("report.html", TEMPLATE)?;

    let score = calculate_score(findings);
    let color = score_color(score);

    // Build findings as serializable values for Tera
    let findings_values: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            let evidence_list: Vec<serde_json::Value> = f
                .evidence
                .iter()
                .map(|ev| {
                    let req_headers: Vec<(String, String)> = ev
                        .request
                        .headers
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    serde_json::json!({
                        "description": ev.description,
                        "request_method": ev.request.method,
                        "request_url": ev.request.url,
                        "request_headers": req_headers,
                        "request_body": ev.request.body.clone().unwrap_or_default(),
                        "response_status": ev.response.status_code,
                        "response_body": ev.response.body,
                    })
                })
                .collect();

            serde_json::json!({
                "title": vuln_type_title(&f.vuln_type),
                "severity": severity_label(&f.severity),
                "severity_class": severity_class(&f.severity),
                "endpoint": f.endpoint,
                "description": f.description,
                "fix_suggestion": f.fix_suggestion,
                "evidence": evidence_list,
            })
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("target_url", &target.base_url.as_deref().unwrap_or("N/A"));
    ctx.insert("scan_date", scan_date);
    ctx.insert("score", &score);
    ctx.insert("score_color", color);
    ctx.insert("total_findings", &findings.len());
    ctx.insert(
        "critical_count",
        &count_severity(findings, &Severity::Critical),
    );
    ctx.insert("high_count", &count_severity(findings, &Severity::High));
    ctx.insert("medium_count", &count_severity(findings, &Severity::Medium));
    ctx.insert("low_count", &count_severity(findings, &Severity::Low));
    ctx.insert("findings", &findings_values);
    ctx.insert("timeline", timeline);

    // Determine effective flag count from KB if available
    let effective_flag_count = knowledge.map(|kb| kb.flags.len()).unwrap_or(flag_count);
    ctx.insert("flag_count", &effective_flag_count);

    // Build goal progress section from KnowledgeBase
    if let Some(kb) = knowledge {
        let goal_type_label = match &kb.goal.goal_type {
            crate::agent::knowledge::GoalType::CaptureFlags { .. } => "Capture Flags",
            crate::agent::knowledge::GoalType::GainAccess { .. } => "Gain Access",
            crate::agent::knowledge::GoalType::Exfiltrate { .. } => "Exfiltrate",
            crate::agent::knowledge::GoalType::VulnerabilityAssessment { .. } => {
                "Vulnerability Assessment"
            }
            crate::agent::knowledge::GoalType::Custom { .. } => "Custom",
        };
        ctx.insert("goal_type", goal_type_label);

        let goal_status_label = match &kb.goal.status {
            crate::agent::knowledge::GoalStatus::InProgress => "In Progress",
            crate::agent::knowledge::GoalStatus::Achieved => "Achieved",
            crate::agent::knowledge::GoalStatus::PartiallyAchieved => "Partially Achieved",
            crate::agent::knowledge::GoalStatus::Failed => "Failed",
        };
        ctx.insert("goal_status", goal_status_label);

        let goal_status_class = match &kb.goal.status {
            crate::agent::knowledge::GoalStatus::InProgress => "inprogress",
            crate::agent::knowledge::GoalStatus::Achieved => "achieved",
            crate::agent::knowledge::GoalStatus::PartiallyAchieved => "partial",
            crate::agent::knowledge::GoalStatus::Failed => "failed",
        };
        ctx.insert("goal_status_class", goal_status_class);

        ctx.insert("goal_description", &kb.goal.description);

        let criteria_values: Vec<serde_json::Value> = kb
            .goal
            .success_criteria
            .iter()
            .map(|c| {
                serde_json::json!({
                    "description": c.description,
                    "met": c.met,
                })
            })
            .collect();
        ctx.insert("goal_criteria", &criteria_values);
    }

    // Build deductive metrics section from KnowledgeBase
    if let Some(kb) = knowledge {
        let metrics = &kb.deductive_metrics;
        let has_metrics = metrics.total_tool_calls > 0 || metrics.hypotheses_generated > 0;
        ctx.insert("has_deductive_metrics", &has_metrics);

        if has_metrics {
            ctx.insert(
                "efficiency_score",
                &format!("{:.0}", metrics.efficiency_score() * 100.0),
            );
            ctx.insert(
                "probe_ratio",
                &format!("{:.0}", metrics.efficiency_score() * 100.0),
            );
            ctx.insert(
                "brute_force_ratio",
                &format!("{:.0}", metrics.brute_force_ratio() * 100.0),
            );
            ctx.insert(
                "confirmation_rate",
                &format!("{:.0}", metrics.confirmation_rate() * 100.0),
            );

            // Hypothesis funnel from system model
            let hypotheses = &kb.system_model.hypotheses;
            let generated = hypotheses.len() as u32;
            let probing = hypotheses
                .iter()
                .filter(|h| h.status == crate::agent::knowledge::HypothesisStatus::Probing)
                .count() as u32;
            let confirmed = hypotheses
                .iter()
                .filter(|h| h.status == crate::agent::knowledge::HypothesisStatus::Confirmed)
                .count() as u32;
            let exploited = hypotheses
                .iter()
                .filter(|h| h.status == crate::agent::knowledge::HypothesisStatus::Exploited)
                .count() as u32;

            ctx.insert("funnel_generated", &generated);
            ctx.insert("funnel_probing", &probing);
            ctx.insert("funnel_confirmed", &confirmed);
            ctx.insert("funnel_exploited", &exploited);

            // Percentage for bar widths (relative to generated, or 100% if 0)
            let max_count = generated.max(1) as f64;
            ctx.insert(
                "funnel_generated_pct",
                &format!("{:.0}", generated as f64 / max_count * 100.0),
            );
            ctx.insert(
                "funnel_probing_pct",
                &format!("{:.0}", probing as f64 / max_count * 100.0),
            );
            ctx.insert(
                "funnel_confirmed_pct",
                &format!("{:.0}", confirmed as f64 / max_count * 100.0),
            );
            ctx.insert(
                "funnel_exploited_pct",
                &format!("{:.0}", exploited as f64 / max_count * 100.0),
            );

            // Historical comparison (if available)
            if let Some(hist) = historical {
                let mut comparisons = Vec::new();

                let eff = metrics.efficiency_score();
                comparisons.push(serde_json::json!({
                    "metric": "Efficiency Score",
                    "current": format!("{:.0}%", eff * 100.0),
                    "historical": format!("{:.0}%", hist.avg_efficiency_score * 100.0),
                    "class": if eff >= hist.avg_efficiency_score { "metric-better" } else { "metric-worse" },
                }));

                let conf = metrics.confirmation_rate();
                comparisons.push(serde_json::json!({
                    "metric": "Confirmation Rate",
                    "current": format!("{:.0}%", conf * 100.0),
                    "historical": format!("{:.0}%", hist.avg_confirmation_rate * 100.0),
                    "class": if conf >= hist.avg_confirmation_rate { "metric-better" } else { "metric-worse" },
                }));

                let tc = metrics.total_tool_calls as f64;
                comparisons.push(serde_json::json!({
                    "metric": "Tool Calls",
                    "current": format!("{}", metrics.total_tool_calls),
                    "historical": format!("{:.0}", hist.avg_tool_calls),
                    "class": if tc <= hist.avg_tool_calls { "metric-better" } else { "metric-worse" },
                }));

                let dur = metrics.wall_clock_secs as f64;
                comparisons.push(serde_json::json!({
                    "metric": "Duration (secs)",
                    "current": format!("{}", metrics.wall_clock_secs),
                    "historical": format!("{:.0}", hist.avg_duration_secs),
                    "class": if dur <= hist.avg_duration_secs { "metric-better" } else { "metric-worse" },
                }));

                ctx.insert("has_historical", &true);
                ctx.insert("historical_comparisons", &comparisons);
            } else {
                ctx.insert("has_historical", &false);
            }
        } else {
            ctx.insert("has_historical", &false);
        }
    } else {
        ctx.insert("has_deductive_metrics", &false);
        ctx.insert("has_historical", &false);
    }

    // Build specialist-related sections from KnowledgeBase
    if let Some(kb) = knowledge {
        // Specialists section
        let specialists_values: Vec<serde_json::Value> = kb
            .activated_specialists
            .iter()
            .map(|name| {
                let count = findings
                    .iter()
                    .filter(|f| f.endpoint.contains(name) || name.contains("recon"))
                    .count();
                serde_json::json!({
                    "name": name,
                    "findings_count": count,
                    "findings_suffix": if count == 1 { "" } else { "s" },
                })
            })
            .collect();
        ctx.insert("specialists", &specialists_values);

        // Credentials table
        let creds_values: Vec<serde_json::Value> = kb
            .credentials
            .iter()
            .map(|c| {
                let secret = c.password.as_deref().or(c.hash.as_deref()).unwrap_or("N/A");
                serde_json::json!({
                    "username": c.username,
                    "secret": secret,
                    "service": if c.service.is_empty() { "—" } else { &c.service },
                    "host": if c.host.is_empty() { "—" } else { &c.host },
                })
            })
            .collect();
        ctx.insert("credentials", &creds_values);

        // Attack paths
        let paths_values: Vec<serde_json::Value> = kb
            .attack_paths
            .iter()
            .map(|p| {
                serde_json::json!({
                    "description": p.description,
                })
            })
            .collect();
        ctx.insert("attack_paths", &paths_values);

        // Flags
        ctx.insert("flags", &kb.flags);
    } else {
        // Empty collections for backward compatibility
        let empty: Vec<serde_json::Value> = vec![];
        ctx.insert("specialists", &empty);
        ctx.insert("credentials", &empty);
        ctx.insert("attack_paths", &empty);
        let empty_strings: Vec<String> = vec![];
        ctx.insert("flags", &empty_strings);
    }

    tera.render("report.html", &ctx)
}

/// Historical averages for comparison in reports.
#[derive(Debug, Clone, Default)]
pub struct HistoricalMetrics {
    pub avg_efficiency_score: f64,
    pub avg_confirmation_rate: f64,
    pub avg_tool_calls: f64,
    pub avg_duration_secs: f64,
}

pub fn generate_html_with_history(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
    flag_count: usize,
    knowledge: Option<&KnowledgeBase>,
    historical: Option<&HistoricalMetrics>,
) -> Result<String, tera::Error> {
    generate_html_with_knowledge_inner(
        findings, target, timeline, scan_date, flag_count, knowledge, historical,
    )
}

pub fn generate_report(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
    output_path: &Path,
) -> Result<String, ReportError> {
    let html = generate_html(findings, target, timeline, scan_date)?;
    fs::write(output_path, &html)?;
    Ok(html)
}

pub fn generate_report_with_knowledge(
    findings: &[Finding],
    target: &Target,
    timeline: &[String],
    scan_date: &str,
    output_path: &Path,
    knowledge: &KnowledgeBase,
) -> Result<String, ReportError> {
    let html =
        generate_html_with_knowledge(findings, target, timeline, scan_date, 0, Some(knowledge))?;
    fs::write(output_path, &html)?;
    Ok(html)
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("Template rendering failed: {0}")]
    TemplateError(#[from] tera::Error),
    #[error("File write failed: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::agent::knowledge::AttackPath;
    use crate::types::{Evidence, HttpRequest, HttpResponse};

    fn mock_finding_sqli() -> Finding {
        Finding {
            vuln_type: VulnType::SqlInjection,
            severity: Severity::Critical,
            endpoint: "/api/users".to_string(),
            evidence: vec![Evidence {
                request: HttpRequest {
                    method: "GET".to_string(),
                    url: "/api/users?id=1' OR '1'='1".to_string(),
                    headers: HashMap::from([("Host".to_string(), "example.com".to_string())]),
                    body: None,
                },
                response: HttpResponse {
                    status_code: 200,
                    headers: HashMap::new(),
                    body: "all users returned".to_string(),
                    elapsed_ms: 150,
                },
                payload: "1' OR '1'='1".to_string(),
                description: "SQL injection in id parameter".to_string(),
            }],
            description: "SQL injection vulnerability in user lookup".to_string(),
            fix_suggestion: "Use parameterized queries".to_string(),
        }
    }

    fn mock_finding_xss() -> Finding {
        Finding {
            vuln_type: VulnType::ReflectedXSS,
            severity: Severity::High,
            endpoint: "/search".to_string(),
            evidence: vec![Evidence {
                request: HttpRequest {
                    method: "GET".to_string(),
                    url: "/search?q=<script>alert(1)</script>".to_string(),
                    headers: HashMap::new(),
                    body: None,
                },
                response: HttpResponse {
                    status_code: 200,
                    headers: HashMap::new(),
                    body: "<html><script>alert(1)</script></html>".to_string(),
                    elapsed_ms: 50,
                },
                payload: "<script>alert(1)</script>".to_string(),
                description: "XSS payload reflected in response".to_string(),
            }],
            description: "Reflected XSS in search parameter".to_string(),
            fix_suggestion: "Escape all user-controlled output. Remove unsafe template directives such as |safe. Consider adding Content-Security-Policy headers.".to_string(),
        }
    }

    #[test]
    fn test_calculate_score_no_findings() {
        assert_eq!(calculate_score(&[]), 100);
    }

    #[test]
    fn test_calculate_score_with_findings() {
        let findings = vec![mock_finding_sqli(), mock_finding_xss()];
        // Critical: -30, High: -20 = 50
        assert_eq!(calculate_score(&findings), 50);
    }

    #[test]
    fn test_calculate_score_floor_at_zero() {
        let findings: Vec<Finding> = (0..10).map(|_| mock_finding_sqli()).collect();
        // 10 * 30 = 300, but min is 0
        assert_eq!(calculate_score(&findings), 0);
    }

    #[test]
    fn test_score_color_ranges() {
        assert_eq!(score_color(0), "#ff4444");
        assert_eq!(score_color(30), "#ff4444");
        assert_eq!(score_color(31), "#ff8c00");
        assert_eq!(score_color(60), "#ff8c00");
        assert_eq!(score_color(61), "#ffd700");
        assert_eq!(score_color(80), "#ffd700");
        assert_eq!(score_color(81), "#4caf50");
        assert_eq!(score_color(100), "#4caf50");
    }

    #[test]
    fn test_fix_suggestion_for_vuln_types() {
        assert_eq!(
            fix_suggestion_for(&VulnType::SqlInjection),
            "Use parameterized queries"
        );
        assert_eq!(
            fix_suggestion_for(&VulnType::ReflectedXSS),
            "Escape all user-controlled output. Remove unsafe template directives such as |safe. Consider adding Content-Security-Policy headers."
        );
        assert_eq!(
            fix_suggestion_for(&VulnType::IDOR),
            "Implement authorization checks verifying resource ownership"
        );
    }

    #[test]
    fn test_generate_html_with_mock_findings() {
        let findings = vec![mock_finding_sqli(), mock_finding_xss()];
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let timeline = vec![
            "Crawled target for endpoints".to_string(),
            "Tested SQL injection on /api/users".to_string(),
            "Tested XSS on /search".to_string(),
        ];

        let html = generate_html(&findings, &target, &timeline, "2026-03-03").unwrap();

        // Verify score (Critical -30 + High -20 = 50)
        assert!(html.contains("50/100"), "Should contain score 50/100");

        // Verify finding titles
        assert!(
            html.contains("SQL Injection"),
            "Should contain SQL Injection title"
        );
        assert!(
            html.contains("Reflected XSS"),
            "Should contain Reflected XSS title"
        );

        // Verify severity badges
        assert!(
            html.contains("badge-critical"),
            "Should contain critical badge"
        );
        assert!(html.contains("badge-high"), "Should contain high badge");

        // Verify endpoints
        assert!(
            html.contains("/api/users"),
            "Should contain /api/users endpoint"
        );
        assert!(html.contains("/search"), "Should contain /search endpoint");

        // Verify target URL
        assert!(
            html.contains("https://example.com"),
            "Should contain target URL"
        );

        // Verify scan date
        assert!(html.contains("2026-03-03"), "Should contain scan date");

        // Verify fix suggestions
        assert!(
            html.contains("Use parameterized queries"),
            "Should contain SQLi fix suggestion"
        );
        assert!(
            html.contains("Escape all user-controlled output. Remove unsafe template directives such as |safe. Consider adding Content-Security-Policy headers."),
            "Should contain XSS fix suggestion"
        );

        // Verify evidence sections
        assert!(
            html.contains("SQL injection in id parameter"),
            "Should contain evidence description"
        );
        assert!(
            html.contains("XSS payload reflected in response"),
            "Should contain XSS evidence description"
        );

        // Verify timeline
        assert!(
            html.contains("Crawled target for endpoints"),
            "Should contain timeline entry"
        );

        // Verify severity counts
        assert!(
            html.contains("severity-critical"),
            "Should contain critical count styling"
        );
        assert!(
            html.contains("severity-high"),
            "Should contain high count styling"
        );

        // Verify it's a self-contained HTML file
        assert!(html.contains("<!DOCTYPE html>"), "Should be valid HTML");
        assert!(html.contains("<style>"), "Should contain inline CSS");
        assert!(
            !html.contains("<link rel=\"stylesheet\""),
            "Should not have external CSS"
        );
    }

    #[test]
    fn test_generate_html_no_findings() {
        let target = Target {
            base_url: Some("https://safe.example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };

        let html = generate_html(&[], &target, &[], "2026-03-03").unwrap();

        assert!(
            html.contains("100/100"),
            "Score should be 100 with no findings"
        );
        assert!(
            html.contains("No vulnerabilities found"),
            "Should show no findings message"
        );
    }

    #[test]
    fn test_generate_report_writes_file() {
        let findings = vec![mock_finding_sqli()];
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };

        let dir = std::env::temp_dir().join("redtrail_test_report");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_report.html");

        let html = generate_report(&findings, &target, &[], "2026-03-03", &path).unwrap();

        assert!(path.exists(), "Report file should exist");
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, html);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    // ----------------------------------------------------------------
    // US-016: Additional report generator tests
    // ----------------------------------------------------------------

    #[test]
    fn test_score_three_criticals_equals_10() {
        // 3 Critical findings: 100 - 3*30 = 10
        let findings: Vec<Finding> = (0..3).map(|_| mock_finding_sqli()).collect();
        assert_eq!(calculate_score(&findings), 10);
    }

    #[test]
    fn test_score_mixed_severities() {
        // 1 Critical (-30) + 1 High (-20) + 1 Medium (-10) = 100-60 = 40
        let findings = vec![
            mock_finding_sqli(), // Critical
            mock_finding_xss(),  // High
            Finding {
                vuln_type: VulnType::InformationDisclosure,
                severity: Severity::Medium,
                endpoint: "/debug".to_string(),
                evidence: vec![],
                description: "debug info".to_string(),
                fix_suggestion: "Remove debug info".to_string(),
            },
        ];
        assert_eq!(calculate_score(&findings), 40);
    }

    #[test]
    fn test_score_with_low_and_info() {
        // 1 Low (-5) + 1 Info (0) = 100-5 = 95
        let findings = vec![
            Finding {
                vuln_type: VulnType::InformationDisclosure,
                severity: Severity::Low,
                endpoint: "/version".to_string(),
                evidence: vec![],
                description: "version info".to_string(),
                fix_suggestion: "remove".to_string(),
            },
            Finding {
                vuln_type: VulnType::InformationDisclosure,
                severity: Severity::Info,
                endpoint: "/health".to_string(),
                evidence: vec![],
                description: "health check".to_string(),
                fix_suggestion: "remove".to_string(),
            },
        ];
        assert_eq!(calculate_score(&findings), 95);
    }

    #[test]
    fn test_evidence_rendered_in_html() {
        let findings = vec![mock_finding_sqli()];
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let html = generate_html(&findings, &target, &[], "2026-03-03").unwrap();

        // Evidence request URL (may be HTML-escaped in some ways)
        assert!(
            html.contains("/api/users"),
            "Should contain evidence request URL base"
        );
        // Evidence response body
        assert!(
            html.contains("all users returned"),
            "Should contain evidence response body"
        );
        // Evidence response status
        assert!(
            html.contains("200"),
            "Should contain evidence response status"
        );
        // Evidence description
        assert!(
            html.contains("SQL injection in id parameter"),
            "Should contain evidence description"
        );
    }

    #[test]
    fn test_fix_suggestion_command_injection() {
        assert_eq!(
            fix_suggestion_for(&VulnType::CommandInjection),
            "Sanitize inputs and avoid passing user data to shell commands"
        );
    }

    #[test]
    fn test_fix_suggestion_info_disclosure() {
        assert_eq!(
            fix_suggestion_for(&VulnType::InformationDisclosure),
            "Remove sensitive data from responses and disable verbose error messages"
        );
    }

    #[test]
    fn test_fix_suggestion_stored_xss() {
        assert_eq!(
            fix_suggestion_for(&VulnType::StoredXSS),
            fix_suggestion_for(&VulnType::ReflectedXSS),
            "StoredXSS and ReflectedXSS should have the same fix suggestion"
        );
    }

    #[test]
    fn test_fix_suggestion_privilege_escalation() {
        assert_eq!(
            fix_suggestion_for(&VulnType::PrivilegeEscalation),
            fix_suggestion_for(&VulnType::IDOR),
            "IDOR and PrivilegeEscalation should have the same fix suggestion"
        );
    }

    #[test]
    fn test_generate_html_single_info_finding() {
        let findings = vec![Finding {
            vuln_type: VulnType::InformationDisclosure,
            severity: Severity::Info,
            endpoint: "/robots.txt".to_string(),
            evidence: vec![],
            description: "Robots.txt exposed".to_string(),
            fix_suggestion: "Review robots.txt".to_string(),
        }];
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let html = generate_html(&findings, &target, &[], "2026-03-03").unwrap();

        // Score should be 100 (Info = 0 deduction)
        assert!(
            html.contains("100/100"),
            "Info finding should not reduce score"
        );
        assert!(html.contains("Information Disclosure"));
        assert!(html.contains("badge-info"));
    }

    #[test]
    fn test_severity_counts_in_html() {
        let findings = vec![
            mock_finding_sqli(), // Critical
            mock_finding_sqli(), // Critical
            mock_finding_xss(),  // High
        ];
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let html = generate_html(&findings, &target, &[], "2026-03-03").unwrap();

        // Verify the HTML contains severity count sections
        assert!(html.contains("severity-critical"));
        assert!(html.contains("severity-high"));
    }

    #[test]
    fn test_html_contains_timeline() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let timeline = vec![
            "Started scan".to_string(),
            "Discovered 5 endpoints".to_string(),
            "Completed in 30s".to_string(),
        ];
        let html = generate_html(&[], &target, &timeline, "2026-03-03").unwrap();

        assert!(html.contains("Started scan"));
        assert!(html.contains("Discovered 5 endpoints"));
        assert!(html.contains("Completed in 30s"));
    }

    #[test]
    fn test_generate_html_with_knowledge_specialists() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.activated_specialists.push("recon".to_string());
        kb.activated_specialists.push("web_exploit".to_string());

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-04", 0, Some(&kb)).unwrap();

        assert!(
            html.contains("Specialists"),
            "Should contain Specialists section"
        );
        assert!(html.contains("recon"), "Should list recon specialist");
        assert!(
            html.contains("web_exploit"),
            "Should list web_exploit specialist"
        );
    }

    #[test]
    fn test_generate_html_with_knowledge_credentials() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.credentials.push(crate::types::Credential {
            username: "admin".to_string(),
            password: Some("secret123".to_string()),
            hash: None,
            service: "ssh".to_string(),
            host: "10.0.0.1".to_string(),
        });
        kb.credentials.push(crate::types::Credential {
            username: "root".to_string(),
            password: None,
            hash: Some("aabbccdd".to_string()),
            service: "smb".to_string(),
            host: "10.0.0.2".to_string(),
        });

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-04", 0, Some(&kb)).unwrap();

        assert!(
            html.contains("Discovered Credentials"),
            "Should contain credentials section"
        );
        assert!(html.contains("admin"), "Should contain username admin");
        assert!(html.contains("secret123"), "Should contain password");
        assert!(html.contains("root"), "Should contain username root");
        assert!(html.contains("aabbccdd"), "Should contain hash");
        assert!(html.contains("ssh"), "Should contain service");
        assert!(html.contains("10.0.0.1"), "Should contain host");
    }

    #[test]
    fn test_generate_html_with_knowledge_attack_paths() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.attack_paths.push(AttackPath {
            description: "Exploited SQLi on /api/users to dump credentials".to_string(),
        });
        kb.attack_paths.push(AttackPath {
            description: "Used dumped creds to SSH into 10.0.0.1".to_string(),
        });

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-04", 0, Some(&kb)).unwrap();

        assert!(
            html.contains("Attack Path"),
            "Should contain Attack Path section"
        );
        assert!(
            html.contains("Exploited SQLi on /api/users to dump credentials"),
            "Should contain first step"
        );
        assert!(
            html.contains("Used dumped creds to SSH into 10.0.0.1"),
            "Should contain second step"
        );
    }

    #[test]
    fn test_generate_html_with_knowledge_flags() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{first_flag}".to_string());
        kb.flags.push("FLAG{second_flag}".to_string());

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-04", 0, Some(&kb)).unwrap();

        assert!(
            html.contains("Captured Flags"),
            "Should contain Captured Flags section"
        );
        assert!(
            html.contains("FLAG{first_flag}"),
            "Should contain first flag"
        );
        assert!(
            html.contains("FLAG{second_flag}"),
            "Should contain second flag"
        );
        // Flag count in summary should reflect KB flags
        assert!(
            html.contains(">2<"),
            "Should show flag count of 2 in summary"
        );
    }

    #[test]
    fn test_generate_html_with_knowledge_backward_compat() {
        // No knowledge base → no specialist sections rendered
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };

        let html = generate_html_with_knowledge(&[], &target, &[], "2026-03-04", 0, None).unwrap();

        assert!(
            !html.contains("Specialists"),
            "Should not contain Specialists section"
        );
        assert!(
            !html.contains("Discovered Credentials"),
            "Should not contain credentials section"
        );
        assert!(
            !html.contains("Attack Path"),
            "Should not contain Attack Path section"
        );
        assert!(
            !html.contains("Captured Flags"),
            "Should not contain Captured Flags section"
        );
    }

    #[test]
    fn test_generate_html_with_knowledge_empty_kb() {
        // Empty knowledge base → no specialist sections rendered
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let kb = KnowledgeBase::new();

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-04", 0, Some(&kb)).unwrap();

        assert!(
            !html.contains("Specialists"),
            "Should not show empty Specialists section"
        );
        assert!(
            !html.contains("Discovered Credentials"),
            "Should not show empty credentials section"
        );
        assert!(
            !html.contains("Attack Path"),
            "Should not show empty Attack Path section"
        );
        assert!(
            !html.contains("Captured Flags"),
            "Should not show empty Captured Flags section"
        );
    }

    #[test]
    fn test_generate_html_with_goal_progress() {
        use crate::agent::knowledge::{
            Criterion, CriterionCheck, GoalStatus, GoalType, SessionGoal,
        };

        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"FLAG\{[^}]+\}".to_string(),
                expected_count: Some(4),
            },
            description: "Capture all 4 flags from the target".to_string(),
            success_criteria: vec![
                Criterion {
                    description: "Capture at least 4 flags".to_string(),
                    check: CriterionCheck::FlagsCaptured { min_count: 4 },
                    met: false,
                },
                Criterion {
                    description: "Find at least 2 vulnerabilities".to_string(),
                    check: CriterionCheck::VulnsFound {
                        min_count: 2,
                        min_severity: "Medium".to_string(),
                    },
                    met: true,
                },
            ],
            status: GoalStatus::PartiallyAchieved,
        };

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-10", 0, Some(&kb)).unwrap();

        assert!(
            html.contains("Session Goal"),
            "Should contain Session Goal heading"
        );
        assert!(
            html.contains("Capture Flags"),
            "Should contain goal type label"
        );
        assert!(
            html.contains("Partially Achieved"),
            "Should contain goal status"
        );
        assert!(
            html.contains("Capture all 4 flags from the target"),
            "Should contain goal description"
        );
        assert!(
            html.contains("Capture at least 4 flags"),
            "Should contain unmet criterion"
        );
        assert!(
            html.contains("Find at least 2 vulnerabilities"),
            "Should contain met criterion"
        );
        assert!(
            html.contains("criteria-met"),
            "Should have met criterion styling"
        );
        assert!(
            html.contains("criteria-unmet"),
            "Should have unmet criterion styling"
        );
        assert!(
            html.contains("goal-status-partial"),
            "Should have partial status class"
        );
    }

    #[test]
    fn test_generate_html_goal_achieved_status() {
        use crate::agent::knowledge::{GoalStatus, GoalType, SessionGoal};

        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::GainAccess {
                target_host: "10.0.0.1".to_string(),
                privilege_level: "root".to_string(),
            },
            description: "Gain root access to target".to_string(),
            success_criteria: vec![],
            status: GoalStatus::Achieved,
        };

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-10", 0, Some(&kb)).unwrap();

        assert!(html.contains("Gain Access"), "Should show Gain Access type");
        assert!(html.contains("Achieved"), "Should show Achieved status");
        assert!(
            html.contains("goal-status-achieved"),
            "Should have achieved status class"
        );
    }

    #[test]
    fn test_generate_html_no_goal_section_without_kb() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };

        let html = generate_html_with_knowledge(&[], &target, &[], "2026-03-10", 0, None).unwrap();

        assert!(
            !html.contains("Session Goal"),
            "Should not show Session Goal without KB"
        );
    }

    #[test]
    fn test_generate_html_deductive_metrics() {
        use crate::agent::knowledge::{
            DeductiveMetrics, GoalStatus, GoalType, Hypothesis, HypothesisCategory,
            HypothesisStatus, SessionGoal,
        };

        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"FLAG\{[^}]+\}".to_string(),
                expected_count: Some(4),
            },
            description: "Capture flags".to_string(),
            success_criteria: vec![],
            status: GoalStatus::InProgress,
        };
        kb.deductive_metrics = DeductiveMetrics {
            total_tool_calls: 100,
            probe_calls: 70,
            brute_force_calls: 10,
            enumeration_calls: 20,
            hypotheses_generated: 8,
            hypotheses_confirmed: 5,
            hypotheses_refuted: 2,
            flags_captured: 3,
            wall_clock_secs: 300,
        };

        // Add hypotheses to system model for funnel
        kb.system_model.hypotheses = vec![
            Hypothesis {
                id: "h1".into(),
                component_id: "web1".into(),
                category: HypothesisCategory::Input,
                statement: "SQLi in login".into(),
                status: HypothesisStatus::Exploited,
                probes: vec![],
                confidence: 0.9,
                task_ids: vec![],
            },
            Hypothesis {
                id: "h2".into(),
                component_id: "web1".into(),
                category: HypothesisCategory::Boundary,
                statement: "IDOR in API".into(),
                status: HypothesisStatus::Confirmed,
                probes: vec![],
                confidence: 0.8,
                task_ids: vec![],
            },
            Hypothesis {
                id: "h3".into(),
                component_id: "web1".into(),
                category: HypothesisCategory::Input,
                statement: "XSS in search".into(),
                status: HypothesisStatus::Probing,
                probes: vec![],
                confidence: 0.5,
                task_ids: vec![],
            },
            Hypothesis {
                id: "h4".into(),
                component_id: "web1".into(),
                category: HypothesisCategory::State,
                statement: "Session fixation".into(),
                status: HypothesisStatus::Proposed,
                probes: vec![],
                confidence: 0.3,
                task_ids: vec![],
            },
        ];

        let html =
            generate_html_with_knowledge(&[], &target, &[], "2026-03-10", 0, Some(&kb)).unwrap();

        assert!(
            html.contains("Deductive Efficiency"),
            "Should contain Deductive Efficiency heading"
        );
        assert!(
            html.contains("Efficiency Score"),
            "Should contain efficiency score metric"
        );
        assert!(html.contains("70%"), "Should show 70% efficiency score");
        assert!(
            html.contains("Probe Ratio"),
            "Should contain probe ratio metric"
        );
        assert!(
            html.contains("Brute Force Ratio"),
            "Should contain brute force ratio metric"
        );
        assert!(
            html.contains("Confirmation Rate"),
            "Should contain confirmation rate metric"
        );
        assert!(
            html.contains("Hypothesis Funnel"),
            "Should contain hypothesis funnel"
        );
        // Funnel counts
        assert!(html.contains(">4<"), "Should show 4 generated hypotheses");
    }

    #[test]
    fn test_generate_html_deductive_metrics_with_historical() {
        use crate::agent::knowledge::{DeductiveMetrics, GoalStatus, GoalType, SessionGoal};

        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"FLAG\{[^}]+\}".to_string(),
                expected_count: None,
            },
            description: "Test".to_string(),
            success_criteria: vec![],
            status: GoalStatus::InProgress,
        };
        kb.deductive_metrics = DeductiveMetrics {
            total_tool_calls: 50,
            probe_calls: 35,
            brute_force_calls: 5,
            enumeration_calls: 10,
            hypotheses_generated: 6,
            hypotheses_confirmed: 4,
            hypotheses_refuted: 1,
            flags_captured: 2,
            wall_clock_secs: 200,
        };

        let hist = HistoricalMetrics {
            avg_efficiency_score: 0.5,
            avg_confirmation_rate: 0.4,
            avg_tool_calls: 80.0,
            avg_duration_secs: 400.0,
        };

        let html =
            generate_html_with_history(&[], &target, &[], "2026-03-10", 0, Some(&kb), Some(&hist))
                .unwrap();

        assert!(
            html.contains("Comparison with Historical Averages"),
            "Should show historical comparison"
        );
        assert!(
            html.contains("comparison-table"),
            "Should have comparison table"
        );
        assert!(html.contains("metric-better"), "Should show better metrics");
    }

    #[test]
    fn test_generate_html_no_deductive_metrics_without_data() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };

        let html = generate_html_with_knowledge(&[], &target, &[], "2026-03-10", 0, None).unwrap();

        assert!(
            !html.contains("Deductive Efficiency"),
            "Should not show deductive metrics without KB"
        );
    }

    #[test]
    fn test_score_color_exact_boundaries() {
        // Test the exact boundary values
        assert_eq!(score_color(0), "#ff4444");
        assert_eq!(score_color(30), "#ff4444");
        assert_eq!(score_color(31), "#ff8c00");
        assert_eq!(score_color(60), "#ff8c00");
        assert_eq!(score_color(61), "#ffd700");
        assert_eq!(score_color(80), "#ffd700");
        assert_eq!(score_color(81), "#4caf50");
        assert_eq!(score_color(100), "#4caf50");
        // Values above 100 should still be green
        assert_eq!(score_color(200), "#4caf50");
    }
}
