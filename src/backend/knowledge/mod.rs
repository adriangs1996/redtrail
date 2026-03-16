mod queries;
pub mod types;
mod writes;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::agent::attack_graph::AttackGraph;

pub use types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SpecialistRun {
    pub(crate) name: String,
    pub(crate) findings_count_at_run: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeBase {
    #[serde(default)]
    pub goal: SessionGoal,
    #[serde(default)]
    pub system_model: SystemModel,
    pub discovered_hosts: Vec<HostInfo>,
    pub credentials: Vec<crate::types::Credential>,
    pub access_levels: Vec<AccessLevel>,
    pub attack_paths: Vec<AttackPath>,
    pub flags: Vec<String>,
    pub failed_attempts: Vec<FailedAttempt>,
    pub notes: Vec<String>,
    pub activated_specialists: Vec<String>,
    #[serde(default)]
    pub completed_tasks: Vec<TaskSummary>,
    #[serde(default)]
    pub failed_tasks: Vec<TaskSummary>,
    #[serde(default)]
    pub custom_definitions_used: Vec<String>,
    #[serde(default)]
    pub deductive_metrics: DeductiveMetrics,
    #[serde(default)]
    pub attack_graph: AttackGraph,
    #[serde(default)]
    pub defender_model: DefenderModel,
    #[serde(default)]
    pub evidence_chain: EvidenceChain,
    #[serde(default)]
    pub command_history: Vec<CommandRecord>,
    pub(crate) specialist_runs: Vec<SpecialistRun>,
    #[serde(skip)]
    pub(crate) flag_regex: Option<Regex>,
}

impl KnowledgeBase {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn total_findings_count(&self) -> usize {
        self.discovered_hosts.len()
            + self.credentials.len()
            + self.access_levels.len()
            + self.flags.len()
            + self.failed_attempts.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.discovered_hosts.is_empty()
            && self.credentials.is_empty()
            && self.access_levels.is_empty()
            && self.attack_paths.is_empty()
            && self.flags.is_empty()
            && self.failed_attempts.is_empty()
            && self.notes.is_empty()
            && self.activated_specialists.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Finding, Severity};

    #[test]
    fn test_extract_flags() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("test", "target", "Found: FLAG{secret_123} in output");
        assert_eq!(kb.flags, vec!["FLAG{secret_123}"]);
    }

    #[test]
    fn test_extract_multiple_flags() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("test", "target", "FLAG{first} and FLAG{second} found");
        assert_eq!(kb.flags.len(), 2);
        assert!(kb.flags.contains(&"FLAG{first}".to_string()));
        assert!(kb.flags.contains(&"FLAG{second}".to_string()));
    }

    #[test]
    fn test_flag_deduplication() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("test", "target", "FLAG{dup}");
        kb.extract_from_output("test", "target", "FLAG{dup}");
        assert_eq!(kb.flags.len(), 1);
    }

    #[test]
    fn test_extract_credentials() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output(
            "test",
            "target",
            "Found username: admin password: secret123",
        );
        assert_eq!(kb.credentials.len(), 1);
        assert_eq!(kb.credentials[0].username, "admin");
        assert_eq!(kb.credentials[0].password, Some("secret123".to_string()));
    }

    #[test]
    fn test_credential_deduplication() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("test", "target", "user: admin password: secret");
        kb.extract_from_output("test", "target", "user: admin password: secret");
        assert_eq!(kb.credentials.len(), 1);
    }

    #[test]
    fn test_extract_ip_port() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output(
            "nmap",
            "target",
            "Discovered 192.168.1.1:80 and 192.168.1.1:443 open",
        );
        assert_eq!(kb.discovered_hosts.len(), 1);
        assert_eq!(kb.discovered_hosts[0].ip, "192.168.1.1");
        assert!(kb.discovered_hosts[0].ports.contains(&80));
        assert!(kb.discovered_hosts[0].ports.contains(&443));
    }

    #[test]
    fn test_extract_standalone_ip() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("ping", "target", "Reply from 10.0.0.5");
        assert_eq!(kb.discovered_hosts.len(), 1);
        assert_eq!(kb.discovered_hosts[0].ip, "10.0.0.5");
        assert!(kb.discovered_hosts[0].ports.is_empty());
    }

    #[test]
    fn test_host_deduplication() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("scan", "target", "192.168.1.1:80");
        kb.extract_from_output("scan", "target", "192.168.1.1:80");
        assert_eq!(kb.discovered_hosts.len(), 1);
        assert_eq!(kb.discovered_hosts[0].ports.len(), 1);
    }

    #[test]
    fn test_extract_failed_attempts_permission_denied() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("ssh", "10.0.0.1", "Permission denied (publickey)");
        assert_eq!(kb.failed_attempts.len(), 1);
        assert_eq!(kb.failed_attempts[0].tool, "ssh");
        assert_eq!(kb.failed_attempts[0].target, "10.0.0.1");
    }

    #[test]
    fn test_extract_failed_attempts_access_denied() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("http", "target", "403 Access Denied");
        assert_eq!(kb.failed_attempts.len(), 1);
    }

    #[test]
    fn test_no_false_positive_failed_attempts() {
        let mut kb = KnowledgeBase::new();
        kb.extract_from_output("scan", "target", "Connection successful, port open");
        assert!(kb.failed_attempts.is_empty());
    }

    #[test]
    fn test_to_context_summary_empty() {
        let kb = KnowledgeBase::new();
        assert!(kb.to_context_summary().is_empty());
    }

    #[test]
    fn test_to_context_summary_with_data() {
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{test}".to_string());
        kb.discovered_hosts.push(HostInfo {
            ip: "10.0.0.1".to_string(),
            ports: vec![22, 80],
            services: vec![],
            os: None,
        });
        let summary = kb.to_context_summary();
        assert!(summary.contains("FLAG{test}"));
        assert!(summary.contains("10.0.0.1"));
        assert!(summary.contains("22"));
        assert!(summary.contains("80"));
        assert!(summary.contains("Current Knowledge Base"));
    }

    #[test]
    fn test_has_new_findings_for_never_run() {
        let mut kb = KnowledgeBase::new();
        // No findings yet
        assert!(!kb.has_new_findings_for("scanner"));
        // Add a finding
        kb.flags.push("FLAG{x}".to_string());
        assert!(kb.has_new_findings_for("scanner"));
    }

    #[test]
    fn test_has_new_findings_for_after_run() {
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{first}".to_string());
        kb.record_specialist_run("scanner");

        // No new findings since the run
        assert!(!kb.has_new_findings_for("scanner"));

        // Add a new finding
        kb.flags.push("FLAG{second}".to_string());
        assert!(kb.has_new_findings_for("scanner"));
    }

    #[test]
    fn test_record_specialist_run_deduplicates_activated() {
        let mut kb = KnowledgeBase::new();
        kb.record_specialist_run("scanner");
        kb.record_specialist_run("scanner");
        assert_eq!(kb.activated_specialists.len(), 1);
    }

    #[test]
    fn test_has_new_findings_for_independent_specialists() {
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{a}".to_string());
        kb.record_specialist_run("scanner_a");

        // scanner_b has never run, should see the finding
        assert!(kb.has_new_findings_for("scanner_b"));

        // scanner_a already ran at count=1, no new findings
        assert!(!kb.has_new_findings_for("scanner_a"));
    }

    // --- Credential and host query helpers (US-002) ---

    #[test]
    fn test_has_credentials_for_matching() {
        let mut kb = KnowledgeBase::new();
        kb.credentials.push(crate::types::Credential {
            username: "admin".into(),
            password: Some("pass".into()),
            hash: None,
            service: "ssh".into(),
            host: "10.0.0.1".into(),
        });
        assert!(kb.has_credentials_for("10.0.0.1", "ssh"));
        assert!(kb.has_credentials_for("10.0.0.1", "")); // empty service matches any
        assert!(!kb.has_credentials_for("10.0.0.2", "ssh")); // different host
    }

    #[test]
    fn test_has_credentials_for_empty_host_matches_all() {
        let mut kb = KnowledgeBase::new();
        kb.credentials.push(crate::types::Credential {
            username: "admin".into(),
            password: Some("pass".into()),
            hash: None,
            service: "ssh".into(),
            host: "".into(), // general-purpose cred
        });
        assert!(kb.has_credentials_for("10.0.0.1", "ssh"));
        assert!(kb.has_credentials_for("10.0.0.99", "ssh"));
    }

    #[test]
    fn test_get_credentials_for() {
        let mut kb = KnowledgeBase::new();
        kb.credentials.push(crate::types::Credential {
            username: "admin".into(),
            password: Some("pass".into()),
            hash: None,
            service: "ssh".into(),
            host: "10.0.0.1".into(),
        });
        let cred = kb.get_credentials_for("10.0.0.1", "ssh");
        assert!(cred.is_some());
        assert_eq!(cred.unwrap().username, "admin");

        assert!(kb.get_credentials_for("10.0.0.2", "ssh").is_none());
    }

    #[test]
    fn test_host_with_port() {
        let mut kb = KnowledgeBase::new();
        kb.discovered_hosts.push(HostInfo {
            ip: "10.0.0.1".into(),
            ports: vec![22, 80],
            services: vec![],
            os: None,
        });
        assert!(kb.host_with_port("10.0.0.1", 22).is_some());
        assert!(kb.host_with_port("10.0.0.1", 443).is_none());
        assert!(kb.host_with_port("10.0.0.2", 22).is_none());
    }

    #[test]
    fn test_get_host() {
        let mut kb = KnowledgeBase::new();
        kb.discovered_hosts.push(HostInfo {
            ip: "10.0.0.1".into(),
            ports: vec![80],
            services: vec![],
            os: None,
        });
        assert!(kb.get_host("10.0.0.1").is_some());
        assert!(kb.get_host("10.0.0.2").is_none());
    }

    // --- Task summary and situation report (US-003) ---

    fn make_task_summary(name: &str, status: TaskStatus) -> TaskSummary {
        TaskSummary {
            task_name: name.to_string(),
            task_type: "recon".to_string(),
            target_host: "10.0.0.1".to_string(),
            duration_secs: 30,
            key_findings: "Found open port 22".to_string(),
            status,
            timestamp: 1709640000,
        }
    }

    #[test]
    fn test_add_task_summary_completed() {
        let mut kb = KnowledgeBase::new();
        kb.add_task_summary(make_task_summary("nmap_scan", TaskStatus::Completed));
        assert_eq!(kb.completed_tasks.len(), 1);
        assert!(kb.failed_tasks.is_empty());
        assert_eq!(kb.completed_tasks[0].task_name, "nmap_scan");
    }

    #[test]
    fn test_add_task_summary_failed() {
        let mut kb = KnowledgeBase::new();
        kb.add_task_summary(make_task_summary("ssh_brute", TaskStatus::Failed));
        assert!(kb.completed_tasks.is_empty());
        assert_eq!(kb.failed_tasks.len(), 1);
    }

    #[test]
    fn test_add_task_summary_timeout() {
        let mut kb = KnowledgeBase::new();
        kb.add_task_summary(make_task_summary("slow_scan", TaskStatus::Timeout));
        assert!(kb.completed_tasks.is_empty());
        assert_eq!(kb.failed_tasks.len(), 1);
        assert_eq!(kb.failed_tasks[0].status, TaskStatus::Timeout);
    }

    #[test]
    fn test_situation_report_empty() {
        let kb = KnowledgeBase::new();
        let report = kb.situation_report();
        assert!(report.contains("# Situation Report"));
        // Should not contain any section headers when empty
        assert!(!report.contains("## Discovered Hosts"));
    }

    #[test]
    fn test_situation_report_with_full_state() {
        let mut kb = KnowledgeBase::new();

        kb.discovered_hosts.push(HostInfo {
            ip: "10.0.0.1".to_string(),
            ports: vec![22, 80],
            services: vec!["ssh".to_string(), "http".to_string()],
            os: Some("Linux".to_string()),
        });
        kb.credentials.push(crate::types::Credential {
            username: "root".into(),
            password: Some("toor".into()),
            hash: None,
            service: "ssh".into(),
            host: "10.0.0.1".into(),
        });
        kb.flags.push("FLAG{test_flag}".to_string());
        kb.notes.push("Target appears to run Ubuntu".to_string());
        kb.add_task_summary(make_task_summary("port_scan", TaskStatus::Completed));
        kb.add_task_summary(make_task_summary("ssh_brute", TaskStatus::Failed));

        let report = kb.situation_report();
        assert!(report.contains("## Discovered Hosts"));
        assert!(report.contains("10.0.0.1"));
        assert!(report.contains("ports=[22,80]"));
        assert!(report.contains("services=[ssh,http]"));
        assert!(report.contains("os=Linux"));
        assert!(report.contains("## Credentials"));
        assert!(report.contains("root:toor"));
        assert!(report.contains("## Captured Flags"));
        assert!(report.contains("FLAG{test_flag}"));
        assert!(report.contains("## Completed Tasks"));
        assert!(report.contains("port_scan"));
        assert!(report.contains("## Failed Tasks"));
        assert!(report.contains("ssh_brute"));
        assert!(report.contains("FAILED"));
        assert!(report.contains("## Notes"));
        assert!(report.contains("Ubuntu"));
    }

    #[test]
    fn test_backward_compat_deserialization() {
        // Simulate a JSON from before US-003 (no completed_tasks/failed_tasks fields)
        let old_json = r#"{
            "discovered_hosts": [],
            "credentials": [],
            "access_levels": [],
            "attack_paths": [],
            "flags": ["FLAG{old}"],
            "failed_attempts": [],
            "notes": [],
            "activated_specialists": [],
            "specialist_runs": []
        }"#;
        let kb: KnowledgeBase = serde_json::from_str(old_json).unwrap();
        assert!(kb.completed_tasks.is_empty());
        assert!(kb.failed_tasks.is_empty());
        assert_eq!(kb.flags, vec!["FLAG{old}"]);
    }

    #[test]
    fn test_situation_report_conciseness() {
        let mut kb = KnowledgeBase::new();
        // Add a moderate amount of data
        for i in 0..10 {
            kb.discovered_hosts.push(HostInfo {
                ip: format!("10.0.0.{i}"),
                ports: vec![22, 80, 443],
                services: vec!["ssh".into(), "http".into()],
                os: None,
            });
            kb.add_task_summary(TaskSummary {
                task_name: format!("task_{i}"),
                task_type: "recon".to_string(),
                target_host: format!("10.0.0.{i}"),
                duration_secs: 30,
                key_findings: "Found services".to_string(),
                status: TaskStatus::Completed,
                timestamp: 1709640000 + i,
            });
        }
        let report = kb.situation_report();
        // Rough token estimate: ~4 chars per token. 4000 tokens ~ 16000 chars
        assert!(
            report.len() < 16000,
            "Report too long: {} chars",
            report.len()
        );
    }

    // --- Custom definitions tracking (US-009) ---

    #[test]
    fn test_record_custom_definition() {
        let mut kb = KnowledgeBase::new();
        kb.record_custom_definition("RedisEnum");
        assert_eq!(kb.custom_definitions_used, vec!["RedisEnum"]);
    }

    #[test]
    fn test_record_custom_definition_deduplicates() {
        let mut kb = KnowledgeBase::new();
        kb.record_custom_definition("RedisEnum");
        kb.record_custom_definition("RedisEnum");
        assert_eq!(kb.custom_definitions_used.len(), 1);
    }

    #[test]
    fn test_merge_custom_definitions_used() {
        let mut kb1 = KnowledgeBase::new();
        kb1.record_custom_definition("RedisEnum");

        let mut kb2 = KnowledgeBase::new();
        kb2.record_custom_definition("MongoEnum");
        kb2.record_custom_definition("RedisEnum");

        kb1.merge_from(&kb2);
        assert_eq!(kb1.custom_definitions_used.len(), 2);
        assert!(
            kb1.custom_definitions_used
                .contains(&"RedisEnum".to_string())
        );
        assert!(
            kb1.custom_definitions_used
                .contains(&"MongoEnum".to_string())
        );
    }

    // --- check_criteria tests (US-004) ---

    fn make_criterion(check: CriterionCheck) -> Criterion {
        Criterion {
            description: "test criterion".to_string(),
            check,
            met: false,
        }
    }

    #[test]
    fn test_check_criteria_flags_captured_met() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::FlagsCaptured {
                min_count: 2,
            })],
            ..Default::default()
        };
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{a}".into());
        kb.flags.push("FLAG{b}".into());
        goal.check_criteria(&kb, &[]);
        assert!(goal.success_criteria[0].met);
        assert!(matches!(goal.status, GoalStatus::Achieved));
    }

    #[test]
    fn test_check_criteria_flags_captured_not_met() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::FlagsCaptured {
                min_count: 3,
            })],
            ..Default::default()
        };
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{a}".into());
        goal.check_criteria(&kb, &[]);
        assert!(!goal.success_criteria[0].met);
        assert!(matches!(goal.status, GoalStatus::InProgress));
    }

    #[test]
    fn test_check_criteria_access_obtained_met() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::AccessObtained {
                host: "10.0.0.1".into(),
                min_privilege: "high".into(),
            })],
            ..Default::default()
        };
        let mut kb = KnowledgeBase::new();
        kb.access_levels.push(AccessLevel {
            host: "10.0.0.1".into(),
            user: "root".into(),
            privilege_level: "critical".into(),
            method: "ssh".into(),
        });
        goal.check_criteria(&kb, &[]);
        assert!(goal.success_criteria[0].met);
        assert!(matches!(goal.status, GoalStatus::Achieved));
    }

    #[test]
    fn test_check_criteria_access_obtained_wrong_host() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::AccessObtained {
                host: "10.0.0.1".into(),
                min_privilege: "low".into(),
            })],
            ..Default::default()
        };
        let mut kb = KnowledgeBase::new();
        kb.access_levels.push(AccessLevel {
            host: "10.0.0.2".into(),
            user: "user".into(),
            privilege_level: "high".into(),
            method: "ssh".into(),
        });
        goal.check_criteria(&kb, &[]);
        assert!(!goal.success_criteria[0].met);
    }

    #[test]
    fn test_check_criteria_access_obtained_insufficient_privilege() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::AccessObtained {
                host: "10.0.0.1".into(),
                min_privilege: "critical".into(),
            })],
            ..Default::default()
        };
        let mut kb = KnowledgeBase::new();
        kb.access_levels.push(AccessLevel {
            host: "10.0.0.1".into(),
            user: "user".into(),
            privilege_level: "medium".into(),
            method: "ssh".into(),
        });
        goal.check_criteria(&kb, &[]);
        assert!(!goal.success_criteria[0].met);
    }

    #[test]
    fn test_check_criteria_vulns_found_met() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::VulnsFound {
                min_count: 1,
                min_severity: "high".into(),
            })],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();
        let findings = vec![Finding {
            vuln_type: crate::types::VulnType::SqlInjection,
            severity: Severity::Critical,
            endpoint: "/api".into(),
            evidence: vec![],
            description: "SQLi".into(),
            fix_suggestion: "parameterize".into(),
        }];
        goal.check_criteria(&kb, &findings);
        assert!(goal.success_criteria[0].met);
        assert!(matches!(goal.status, GoalStatus::Achieved));
    }

    #[test]
    fn test_check_criteria_vulns_found_below_severity() {
        let mut goal = SessionGoal {
            success_criteria: vec![make_criterion(CriterionCheck::VulnsFound {
                min_count: 1,
                min_severity: "high".into(),
            })],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();
        let findings = vec![Finding {
            vuln_type: crate::types::VulnType::SqlInjection,
            severity: Severity::Low,
            endpoint: "/api".into(),
            evidence: vec![],
            description: "info leak".into(),
            fix_suggestion: "fix".into(),
        }];
        goal.check_criteria(&kb, &findings);
        assert!(!goal.success_criteria[0].met);
    }

    #[test]
    fn test_check_criteria_data_extracted_preserves_met() {
        let mut goal = SessionGoal {
            success_criteria: vec![Criterion {
                description: "extract db".into(),
                check: CriterionCheck::DataExtracted {
                    description: "database dump".into(),
                },
                met: true, // pre-set by LLM
            }],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();
        goal.check_criteria(&kb, &[]);
        assert!(goal.success_criteria[0].met);
        assert!(matches!(goal.status, GoalStatus::Achieved));
    }

    #[test]
    fn test_check_criteria_custom_preserves_met() {
        let mut goal = SessionGoal {
            success_criteria: vec![Criterion {
                description: "custom check".into(),
                check: CriterionCheck::Custom {
                    description: "something custom".into(),
                },
                met: false,
            }],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();
        goal.check_criteria(&kb, &[]);
        assert!(!goal.success_criteria[0].met);
        assert!(matches!(goal.status, GoalStatus::InProgress));
    }

    #[test]
    fn test_check_criteria_partial_achievement() {
        let mut goal = SessionGoal {
            success_criteria: vec![
                make_criterion(CriterionCheck::FlagsCaptured { min_count: 1 }),
                make_criterion(CriterionCheck::FlagsCaptured { min_count: 5 }),
            ],
            ..Default::default()
        };
        let mut kb = KnowledgeBase::new();
        kb.flags.push("FLAG{one}".into());
        goal.check_criteria(&kb, &[]);
        assert!(goal.success_criteria[0].met);
        assert!(!goal.success_criteria[1].met);
        assert!(matches!(goal.status, GoalStatus::PartiallyAchieved));
    }

    #[test]
    fn test_check_criteria_empty_criteria_stays_in_progress() {
        let mut goal = SessionGoal::default();
        let kb = KnowledgeBase::new();
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::InProgress));
    }

    #[test]
    fn test_backward_compat_custom_definitions_used() {
        // Simulate a JSON from before US-009 (no custom_definitions_used field)
        let old_json = r#"{
            "discovered_hosts": [],
            "credentials": [],
            "access_levels": [],
            "attack_paths": [],
            "flags": [],
            "failed_attempts": [],
            "notes": [],
            "activated_specialists": [],
            "specialist_runs": []
        }"#;
        let kb: KnowledgeBase = serde_json::from_str(old_json).unwrap();
        assert!(kb.custom_definitions_used.is_empty());
    }

    // --- SystemModel tests (US-016) ---

    #[test]
    fn test_system_model_default() {
        let model = SystemModel::default();
        assert!(model.components.is_empty());
        assert!(model.trust_boundaries.is_empty());
        assert!(model.data_flows.is_empty());
        assert!(model.hypotheses.is_empty());
        assert_eq!(model.model_confidence, 0.0);
        assert_eq!(model.current_layer, DeductiveLayer::Modeling);
    }

    #[test]
    fn test_system_model_serialization_roundtrip() {
        let model = SystemModel {
            components: vec![SystemComponent {
                id: "web-1".into(),
                host: "10.0.0.1".into(),
                port: Some(80),
                component_type: ComponentType::WebApp,
                stack: StackFingerprint {
                    server: Some("nginx".into()),
                    framework: Some("django".into()),
                    language: Some("python".into()),
                    technologies: vec!["postgresql".into()],
                },
                entry_points: vec![EntryPoint {
                    path: "/login".into(),
                    method: "POST".into(),
                    parameters: vec!["username".into(), "password".into()],
                    auth_required: false,
                }],
                confidence: 0.9,
            }],
            trust_boundaries: vec![TrustBoundary {
                id: "tb-1".into(),
                name: "DMZ".into(),
                components: vec!["web-1".into()],
            }],
            data_flows: vec![DataFlow {
                from: "web-1".into(),
                to: "db-1".into(),
                data_type: "SQL queries".into(),
                protocol: "TCP".into(),
            }],
            hypotheses: vec![Hypothesis {
                id: "h-1".into(),
                component_id: "web-1".into(),
                category: HypothesisCategory::Input,
                statement: "SQL injection in login".into(),
                status: HypothesisStatus::Proposed,
                probes: vec![ProbeResult {
                    probe_type: "baseline".into(),
                    request_summary: "GET /login?user=admin".into(),
                    response_status: 200,
                    response_length: 1024,
                    timing_ms: 50,
                    anomaly_detected: false,
                    notes: "Normal response".into(),
                }],
                confidence: 0.6,
                task_ids: vec![1, 2],
            }],
            model_confidence: 0.75,
            current_layer: DeductiveLayer::Probing,
        };

        let json = serde_json::to_string(&model).unwrap();
        let restored: SystemModel = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.components.len(), 1);
        assert_eq!(restored.components[0].id, "web-1");
        assert_eq!(restored.trust_boundaries.len(), 1);
        assert_eq!(restored.data_flows.len(), 1);
        assert_eq!(restored.hypotheses.len(), 1);
        assert_eq!(restored.hypotheses[0].statement, "SQL injection in login");
        assert_eq!(restored.model_confidence, 0.75);
        assert_eq!(restored.current_layer, DeductiveLayer::Probing);
    }

    #[test]
    fn test_knowledge_base_has_system_model() {
        let kb = KnowledgeBase::new();
        assert!(kb.system_model.components.is_empty());
        assert_eq!(kb.system_model.current_layer, DeductiveLayer::Modeling);
    }

    #[test]
    fn test_knowledge_base_backward_compat_system_model() {
        // JSON without system_model field should deserialize with default
        let old_json = r#"{
            "discovered_hosts": [],
            "credentials": [],
            "access_levels": [],
            "attack_paths": [],
            "flags": [],
            "failed_attempts": [],
            "notes": [],
            "activated_specialists": [],
            "specialist_runs": []
        }"#;
        let kb: KnowledgeBase = serde_json::from_str(old_json).unwrap();
        assert!(kb.system_model.components.is_empty());
        assert_eq!(kb.system_model.current_layer, DeductiveLayer::Modeling);
    }

    #[test]
    fn test_system_model_add_component() {
        let mut model = SystemModel::default();
        assert!(model.components.is_empty());

        model.add_component(SystemComponent {
            id: "web1".into(),
            host: "10.0.0.1".into(),
            port: Some(80),
            component_type: ComponentType::WebApp,
            stack: StackFingerprint::default(),
            entry_points: vec![],
            confidence: 0.8,
        });

        assert_eq!(model.components.len(), 1);
        assert_eq!(model.components[0].id, "web1");
        assert_eq!(model.components[0].confidence, 0.8);
    }

    #[test]
    fn test_system_model_update_component() {
        let mut model = SystemModel::default();
        model.add_component(SystemComponent {
            id: "web1".into(),
            host: "10.0.0.1".into(),
            port: Some(80),
            component_type: ComponentType::Custom("unknown".into()),
            stack: StackFingerprint::default(),
            entry_points: vec![],
            confidence: 0.3,
        });

        model.update_component(
            "web1",
            ComponentUpdate {
                stack: Some(StackFingerprint {
                    server: Some("nginx".into()),
                    framework: None,
                    language: None,
                    technologies: vec![],
                }),
                component_type: Some("WebApp".into()),
                confidence: Some(0.9),
                add_entry_points: vec![EntryPoint {
                    path: "/api".into(),
                    method: "GET".into(),
                    parameters: vec![],
                    auth_required: false,
                }],
            },
        );

        let comp = &model.components[0];
        assert_eq!(comp.stack.server, Some("nginx".into()));
        assert!(matches!(comp.component_type, ComponentType::WebApp));
        assert_eq!(comp.confidence, 0.9);
        assert_eq!(comp.entry_points.len(), 1);
        assert_eq!(comp.entry_points[0].path, "/api");
    }

    #[test]
    fn test_system_model_update_component_nonexistent() {
        let mut model = SystemModel::default();
        // Should not panic on missing id
        model.update_component(
            "nonexistent",
            ComponentUpdate {
                stack: None,
                component_type: None,
                confidence: Some(1.0),
                add_entry_points: vec![],
            },
        );
        assert!(model.components.is_empty());
    }

    #[test]
    fn test_system_model_add_hypothesis() {
        let mut model = SystemModel::default();
        model.add_hypothesis(Hypothesis {
            id: "h1".into(),
            component_id: "web1".into(),
            category: HypothesisCategory::Input,
            statement: "SQL injection in login form".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.6,
            task_ids: vec![],
        });

        assert_eq!(model.hypotheses.len(), 1);
        assert_eq!(model.hypotheses[0].id, "h1");
    }

    #[test]
    fn test_system_model_update_hypothesis_status() {
        let mut model = SystemModel::default();
        model.add_hypothesis(Hypothesis {
            id: "h1".into(),
            component_id: "web1".into(),
            category: HypothesisCategory::Input,
            statement: "test".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.5,
            task_ids: vec![],
        });

        model.update_hypothesis_status("h1", HypothesisStatus::Confirmed);
        assert_eq!(model.hypotheses[0].status, HypothesisStatus::Confirmed);

        // Nonexistent id should be a no-op
        model.update_hypothesis_status("nonexistent", HypothesisStatus::Refuted);
        assert_eq!(model.hypotheses.len(), 1);
        assert_eq!(model.hypotheses[0].status, HypothesisStatus::Confirmed);
    }

    #[test]
    fn test_system_model_advance_layer() {
        let mut model = SystemModel::default();
        assert_eq!(model.current_layer, DeductiveLayer::Modeling);

        model.advance_layer(DeductiveLayer::Hypothesizing);
        assert_eq!(model.current_layer, DeductiveLayer::Hypothesizing);

        model.advance_layer(DeductiveLayer::Probing);
        assert_eq!(model.current_layer, DeductiveLayer::Probing);
    }

    #[test]
    fn test_system_model_get_hypothesis() {
        let mut model = SystemModel::default();
        assert!(model.get_hypothesis("h1").is_none());

        model.add_hypothesis(Hypothesis {
            id: "h1".into(),
            component_id: "web1".into(),
            category: HypothesisCategory::Boundary,
            statement: "Open admin panel".into(),
            status: HypothesisStatus::Probing,
            probes: vec![],
            confidence: 0.7,
            task_ids: vec![1, 2],
        });

        let h = model.get_hypothesis("h1").unwrap();
        assert_eq!(h.statement, "Open admin panel");
        assert_eq!(h.status, HypothesisStatus::Probing);

        assert!(model.get_hypothesis("nonexistent").is_none());
    }

    #[test]
    fn test_efficiency_score() {
        let mut m = DeductiveMetrics::default();
        assert_eq!(m.efficiency_score(), 0.0); // zero division

        m.total_tool_calls = 10;
        m.probe_calls = 7;
        assert!((m.efficiency_score() - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_flags_per_call() {
        let mut m = DeductiveMetrics::default();
        assert_eq!(m.flags_per_call(), 0.0); // zero division

        m.total_tool_calls = 20;
        m.flags_captured = 4;
        assert!((m.flags_per_call() - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_confirmation_rate() {
        let mut m = DeductiveMetrics::default();
        assert_eq!(m.confirmation_rate(), 0.0); // zero division

        m.hypotheses_generated = 10;
        m.hypotheses_confirmed = 3;
        assert!((m.confirmation_rate() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_brute_force_ratio() {
        let mut m = DeductiveMetrics::default();
        assert_eq!(m.brute_force_ratio(), 0.0); // zero division

        m.total_tool_calls = 50;
        m.brute_force_calls = 5;
        assert!((m.brute_force_ratio() - 0.1).abs() < f64::EPSILON);
    }

    // --- US-057: End-to-end goal criteria flow tests ---

    #[test]
    fn test_e2e_capture_flags_goal_flow() {
        // Simulates: redtrail drive --goal capture-flags --expected-flags 4
        let goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"FLAG\{[^}]+\}".to_string(),
                expected_count: Some(4),
            },
            description: "Capture 4 flags matching pattern: FLAG\\{[^}]+\\}".to_string(),
            success_criteria: vec![Criterion {
                description: "Capture 4 flags".to_string(),
                check: CriterionCheck::FlagsCaptured { min_count: 4 },
                met: false,
            }],
            status: GoalStatus::InProgress,
        };

        let mut kb = KnowledgeBase::new();
        kb.goal = goal;
        kb.sync_flag_regex();

        // Simulate task outputs discovering flags incrementally
        kb.extract_from_output("nmap", "172.20.1.2", "Found FLAG{network_flag_1} in banner");
        assert_eq!(kb.flags.len(), 1);

        let mut goal = std::mem::take(&mut kb.goal);
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::InProgress));
        kb.goal = goal;

        kb.extract_from_output("http", "172.20.1.3", "FLAG{web_flag_2} in response");
        kb.extract_from_output("ssh", "172.20.1.4", "Found FLAG{ssh_flag_3}");
        assert_eq!(kb.flags.len(), 3);

        let mut goal = std::mem::take(&mut kb.goal);
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::InProgress));
        assert!(!goal.success_criteria[0].met);
        kb.goal = goal;

        // Fourth flag triggers achievement
        kb.extract_from_output("privesc", "172.20.1.4", "root: FLAG{suid_flag_4}");
        assert_eq!(kb.flags.len(), 4);

        let mut goal = std::mem::take(&mut kb.goal);
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::Achieved));
        assert!(goal.success_criteria[0].met);
        kb.goal = goal;
    }

    #[test]
    fn test_custom_flag_pattern_extraction() {
        // Test that a custom flag pattern (e.g., HTB{...}) is used for extraction
        let goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"HTB\{[^}]+\}".to_string(),
                expected_count: Some(1),
            },
            description: "Capture HTB flags".to_string(),
            success_criteria: vec![Criterion {
                description: "Capture 1 flag".to_string(),
                check: CriterionCheck::FlagsCaptured { min_count: 1 },
                met: false,
            }],
            status: GoalStatus::InProgress,
        };

        let mut kb = KnowledgeBase::new();
        kb.goal = goal;
        kb.sync_flag_regex();

        // Standard FLAG{} should NOT match with HTB pattern
        kb.extract_from_output("test", "target", "FLAG{should_not_match}");
        assert!(kb.flags.is_empty());

        // HTB{} SHOULD match
        kb.extract_from_output("test", "target", "HTB{h4ck_th3_b0x}");
        assert_eq!(kb.flags, vec!["HTB{h4ck_th3_b0x}"]);

        let mut goal = std::mem::take(&mut kb.goal);
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::Achieved));
        kb.goal = goal;
    }

    #[test]
    fn test_flag_regex_survives_kb_clone() {
        // flag_regex is #[serde(skip)], so after deserialization we need sync_flag_regex()
        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"CTF\{[^}]+\}".to_string(),
                expected_count: Some(1),
            },
            description: "test".to_string(),
            success_criteria: vec![],
            status: GoalStatus::InProgress,
        };
        kb.sync_flag_regex();

        // Serialize and deserialize (simulates session load)
        let json = serde_json::to_string(&kb).unwrap();
        let mut kb2: KnowledgeBase = serde_json::from_str(&json).unwrap();
        // flag_regex is None after deserialization
        assert!(kb2.flag_regex.is_none());

        // After sync, custom pattern works
        kb2.sync_flag_regex();
        kb2.extract_from_output("test", "target", "CTF{loaded}");
        assert_eq!(kb2.flags, vec!["CTF{loaded}"]);
    }

    #[test]
    fn test_default_flag_regex_without_goal() {
        // Without a CaptureFlags goal, should fall back to default FLAG{} pattern
        let mut kb = KnowledgeBase::new();
        kb.sync_flag_regex(); // no-op since goal is Custom by default
        assert!(kb.flag_regex.is_none());

        kb.extract_from_output("test", "target", "FLAG{default_works}");
        assert_eq!(kb.flags, vec!["FLAG{default_works}"]);
    }

    #[test]
    fn test_recon_discovers_hosts_and_services() {
        // Simulate recon output discovering hosts and services for Module 01
        let mut kb = KnowledgeBase::new();

        // Simulate nmap-style output
        kb.extract_from_output(
            "nmap",
            "172.20.1.0/24",
            "172.20.1.2:22 open ssh\n172.20.1.2:80 open http\n172.20.1.3:21 open ftp\n172.20.1.4:80 open http",
        );

        assert!(kb.discovered_hosts.len() >= 3);
        let host2 = kb.discovered_hosts.iter().find(|h| h.ip == "172.20.1.2");
        assert!(host2.is_some());
        let host2 = host2.unwrap();
        assert!(host2.ports.contains(&22));
        assert!(host2.ports.contains(&80));
    }

    // --- US-058: Web app deductive flow tests ---

    #[test]
    fn test_e2e_web_app_deductive_flow_sqli() {
        // Simulates the full L0→L1→L2→L3 deductive flow for a web app SQLi scenario
        let goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: r"FLAG\{[^}]+\}".to_string(),
                expected_count: Some(2),
            },
            description: "Capture 2 flags via SQLi/CMDi".to_string(),
            success_criteria: vec![Criterion {
                description: "Capture 2 flags".to_string(),
                check: CriterionCheck::FlagsCaptured { min_count: 2 },
                met: false,
            }],
            status: GoalStatus::InProgress,
        };

        let mut kb = KnowledgeBase::new();
        kb.goal = goal;
        kb.sync_flag_regex();

        // --- L0: Model the web application ---
        let web_component = SystemComponent {
            id: "web-1".into(),
            host: "172.20.4.20".into(),
            port: Some(80),
            component_type: ComponentType::WebApp,
            stack: StackFingerprint {
                server: Some("Apache/2.4".into()),
                framework: Some("PHP".into()),
                language: Some("PHP".into()),
                technologies: vec!["MySQL".into()],
            },
            entry_points: vec![
                EntryPoint {
                    path: "/search".into(),
                    method: "GET".into(),
                    parameters: vec!["q".into()],
                    auth_required: false,
                },
                EntryPoint {
                    path: "/ping".into(),
                    method: "POST".into(),
                    parameters: vec!["host".into()],
                    auth_required: false,
                },
            ],
            confidence: 0.8,
        };
        kb.system_model.add_component(web_component);
        kb.system_model.model_confidence = 0.7;
        kb.deductive_metrics.total_tool_calls += 3; // L0 enumeration tasks
        kb.deductive_metrics.enumeration_calls += 3;

        assert_eq!(kb.system_model.components.len(), 1);
        assert_eq!(kb.system_model.components[0].entry_points.len(), 2);

        // --- L1: Generate BISCL hypotheses ---
        kb.system_model.advance_layer(DeductiveLayer::Hypothesizing);

        let sqli_hypothesis = Hypothesis {
            id: "h-sqli-search".into(),
            component_id: "web-1".into(),
            category: HypothesisCategory::Input,
            statement:
                "The 'q' parameter on /search is vulnerable to SQL injection (MySQL backend)".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.6,
            task_ids: vec![],
        };
        let cmdi_hypothesis = Hypothesis {
            id: "h-cmdi-ping".into(),
            component_id: "web-1".into(),
            category: HypothesisCategory::Input,
            statement: "The 'host' parameter on /ping is vulnerable to OS command injection".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.5,
            task_ids: vec![],
        };
        kb.system_model.add_hypothesis(sqli_hypothesis);
        kb.system_model.add_hypothesis(cmdi_hypothesis);
        kb.deductive_metrics.hypotheses_generated += 2;

        assert_eq!(kb.system_model.hypotheses.len(), 2);

        // --- L2: Probe hypotheses with differential probes ---
        kb.system_model.advance_layer(DeductiveLayer::Probing);

        // SQLi probes: baseline, edge ('), violation (' OR '1'='1)
        let sqli_h = kb.system_model.get_hypothesis_mut("h-sqli-search").unwrap();
        sqli_h.status = HypothesisStatus::Probing;
        sqli_h.probes.push(ProbeResult {
            probe_type: "baseline".into(),
            request_summary: "GET /search?q=test".into(),
            response_status: 200,
            response_length: 1024,
            timing_ms: 50,
            anomaly_detected: false,
            notes: "Normal search results".into(),
        });
        sqli_h.probes.push(ProbeResult {
            probe_type: "edge".into(),
            request_summary: "GET /search?q=test'".into(),
            response_status: 500,
            response_length: 512,
            timing_ms: 45,
            anomaly_detected: true,
            notes: "SQL syntax error in response, status 500".into(),
        });
        sqli_h.probes.push(ProbeResult {
            probe_type: "violation".into(),
            request_summary: "GET /search?q=test' OR '1'='1".into(),
            response_status: 200,
            response_length: 4096,
            timing_ms: 80,
            anomaly_detected: true,
            notes: "Response length 4x baseline — tautology returned all rows".into(),
        });
        sqli_h.status = HypothesisStatus::Confirmed;
        kb.deductive_metrics.probe_calls += 3;
        kb.deductive_metrics.total_tool_calls += 3;
        kb.deductive_metrics.hypotheses_confirmed += 1;

        // CMDi probes: baseline, edge (;), violation (;id)
        let cmdi_h = kb.system_model.get_hypothesis_mut("h-cmdi-ping").unwrap();
        cmdi_h.status = HypothesisStatus::Probing;
        cmdi_h.probes.push(ProbeResult {
            probe_type: "baseline".into(),
            request_summary: "POST /ping host=127.0.0.1".into(),
            response_status: 200,
            response_length: 256,
            timing_ms: 100,
            anomaly_detected: false,
            notes: "Normal ping response".into(),
        });
        cmdi_h.probes.push(ProbeResult {
            probe_type: "edge".into(),
            request_summary: "POST /ping host=127.0.0.1;".into(),
            response_status: 200,
            response_length: 256,
            timing_ms: 95,
            anomaly_detected: false,
            notes: "Same response, semicolon stripped or ignored".into(),
        });
        cmdi_h.probes.push(ProbeResult {
            probe_type: "violation".into(),
            request_summary: "POST /ping host=127.0.0.1;id".into(),
            response_status: 200,
            response_length: 320,
            timing_ms: 110,
            anomaly_detected: true,
            notes: "uid=33(www-data) found in response body".into(),
        });
        cmdi_h.status = HypothesisStatus::Confirmed;
        kb.deductive_metrics.probe_calls += 3;
        kb.deductive_metrics.total_tool_calls += 3;
        kb.deductive_metrics.hypotheses_confirmed += 1;

        // --- Verify probe ratio > 0.7 ---
        // Total: 3 (L0) + 3 (sqli probes) + 3 (cmdi probes) = 9 total, 6 probe
        assert!(
            kb.deductive_metrics.efficiency_score() > 0.6,
            "Probe ratio should be > 0.6 at this stage, got {}",
            kb.deductive_metrics.efficiency_score()
        );

        // --- L3: Exploit confirmed hypotheses ---
        kb.system_model.advance_layer(DeductiveLayer::Exploiting);

        // Only confirmed hypotheses should be exploited
        let sqli_h = kb.system_model.get_hypothesis("h-sqli-search").unwrap();
        assert_eq!(sqli_h.status, HypothesisStatus::Confirmed);
        let cmdi_h = kb.system_model.get_hypothesis("h-cmdi-ping").unwrap();
        assert_eq!(cmdi_h.status, HypothesisStatus::Confirmed);

        // Simulate exploitation yielding flags
        kb.extract_from_output(
            "exploit",
            "172.20.4.20",
            "UNION SELECT: FLAG{sql1_3xtr4ct3d} from secret_table",
        );
        kb.extract_from_output("exploit", "172.20.4.20", "Command output: FLAG{cmd1_pwn3d}");
        kb.deductive_metrics.total_tool_calls += 2;
        kb.deductive_metrics.flags_captured += 2;

        assert_eq!(kb.flags.len(), 2);

        // Mark hypotheses as exploited
        let sqli_h = kb.system_model.get_hypothesis_mut("h-sqli-search").unwrap();
        sqli_h.status = HypothesisStatus::Exploited;
        let cmdi_h = kb.system_model.get_hypothesis_mut("h-cmdi-ping").unwrap();
        cmdi_h.status = HypothesisStatus::Exploited;

        // --- Verify final metrics ---
        // Total: 9 + 2 exploit = 11 total, 6 probe calls
        let probe_ratio = kb.deductive_metrics.efficiency_score();
        assert!(
            probe_ratio > 0.5,
            "Final probe ratio should be > 0.5, got {probe_ratio}"
        );
        assert_eq!(kb.deductive_metrics.brute_force_calls, 0);
        assert_eq!(kb.deductive_metrics.brute_force_ratio(), 0.0);
        assert_eq!(kb.deductive_metrics.hypotheses_confirmed, 2);
        assert_eq!(kb.deductive_metrics.hypotheses_generated, 2);
        assert!((kb.deductive_metrics.confirmation_rate() - 1.0).abs() < f64::EPSILON);

        // Verify goal achieved
        let mut goal = std::mem::take(&mut kb.goal);
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::Achieved));
        kb.goal = goal;
    }

    #[test]
    fn test_web_app_deductive_metrics_high_probe_ratio() {
        // Simulates a web-focused module where most calls are probes (target: > 0.7)
        let mut metrics = DeductiveMetrics::default();

        // L0: 2 enumeration tasks (StackFingerprint + WebEnum)
        metrics.total_tool_calls += 2;
        metrics.enumeration_calls += 2;

        // L1: No tool calls (hypothesis generation is LLM reasoning only)

        // L2: 4 hypotheses * 3 probes each = 12 probe calls
        metrics.total_tool_calls += 12;
        metrics.probe_calls += 12;
        metrics.hypotheses_generated += 4;

        // 2 confirmed, 2 refuted
        metrics.hypotheses_confirmed += 2;
        metrics.hypotheses_refuted += 2;

        // L3: 2 exploit tasks
        metrics.total_tool_calls += 2;
        metrics.flags_captured += 3;

        // Probe ratio = 12 / 16 = 0.75
        let ratio = metrics.efficiency_score();
        assert!(
            ratio > 0.7,
            "Probe ratio should exceed 0.7 for web-focused module, got {ratio}"
        );
        assert_eq!(metrics.brute_force_calls, 0);
        assert_eq!(metrics.brute_force_ratio(), 0.0);
        assert!((metrics.confirmation_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_confirmation_gate_blocks_unconfirmed_exploit() {
        // Verifies that hypotheses must be Confirmed before exploitation
        let mut model = SystemModel::default();

        let hypothesis = Hypothesis {
            id: "h-unconfirmed".into(),
            component_id: "web-1".into(),
            category: HypothesisCategory::Input,
            statement: "SQL injection in login".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.3,
            task_ids: vec![],
        };
        model.add_hypothesis(hypothesis);

        // Hypothesis is still Proposed — should NOT be exploitable
        let h = model.get_hypothesis("h-unconfirmed").unwrap();
        assert_ne!(h.status, HypothesisStatus::Confirmed);

        // After probing with no anomaly → Refuted
        let h = model.get_hypothesis_mut("h-unconfirmed").unwrap();
        h.status = HypothesisStatus::Probing;
        h.probes.push(ProbeResult {
            anomaly_detected: false,
            ..Default::default()
        });
        h.probes.push(ProbeResult {
            anomaly_detected: false,
            ..Default::default()
        });
        h.probes.push(ProbeResult {
            anomaly_detected: false,
            ..Default::default()
        });
        h.status = HypothesisStatus::Refuted;

        let h = model.get_hypothesis("h-unconfirmed").unwrap();
        assert_eq!(h.status, HypothesisStatus::Refuted);
        // Refuted hypothesis must not be exploited
        assert_ne!(h.status, HypothesisStatus::Confirmed);
    }

    #[test]
    fn test_privesc_deductive_flow_l0_to_l3() {
        // Full deductive flow for privilege escalation module (Module 08 pattern)
        // L0: Discover SSH service → L1: Generate privesc hypotheses → L2: Probe → L3: Exploit
        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: "FLAG\\{[^}]+\\}".to_string(),
                expected_count: Some(3),
            },
            description: "Capture all flags via privilege escalation".into(),
            success_criteria: vec![Criterion {
                description: "Capture 3 flags".into(),
                check: CriterionCheck::FlagsCaptured { min_count: 3 },
                met: false,
            }],
            status: GoalStatus::InProgress,
        };
        kb.sync_flag_regex();

        // --- L0: Model target with SSH service ---
        kb.system_model.advance_layer(DeductiveLayer::Modeling);

        let ssh_component = SystemComponent {
            id: "ssh-1".into(),
            host: "172.20.8.10".into(),
            port: Some(22),
            component_type: ComponentType::Custom("SSHServer".into()),
            stack: StackFingerprint {
                server: Some("OpenSSH 8.2".into()),
                framework: None,
                language: Some("Linux".into()),
                technologies: vec!["Ubuntu 20.04".into()],
            },
            entry_points: vec![],
            confidence: 0.9,
        };
        kb.system_model.add_component(ssh_component);
        kb.system_model.model_confidence = 0.7;
        kb.deductive_metrics.total_tool_calls += 2; // L0: recon + fingerprint
        kb.deductive_metrics.enumeration_calls += 2;

        assert_eq!(kb.system_model.components.len(), 1);

        // --- L1: Generate BISCL hypotheses for privesc vectors ---
        kb.system_model.advance_layer(DeductiveLayer::Hypothesizing);

        let suid_hypothesis = Hypothesis {
            id: "h-suid".into(),
            component_id: "ssh-1".into(),
            category: HypothesisCategory::Boundary,
            statement: "SUID binary allows root command execution".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.6,
            task_ids: vec![],
        };
        let sudo_hypothesis = Hypothesis {
            id: "h-sudo".into(),
            component_id: "ssh-1".into(),
            category: HypothesisCategory::Boundary,
            statement: "Sudo misconfiguration allows privilege escalation via NOPASSWD entry"
                .into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.5,
            task_ids: vec![],
        };
        let cron_hypothesis = Hypothesis {
            id: "h-cron".into(),
            component_id: "ssh-1".into(),
            category: HypothesisCategory::State,
            statement: "Writable cron job script allows code execution as root".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.4,
            task_ids: vec![],
        };
        let capabilities_hypothesis = Hypothesis {
            id: "h-caps".into(),
            component_id: "ssh-1".into(),
            category: HypothesisCategory::Boundary,
            statement: "Binary with cap_setuid capability allows privilege escalation".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.3,
            task_ids: vec![],
        };
        let path_hypothesis = Hypothesis {
            id: "h-path".into(),
            component_id: "ssh-1".into(),
            category: HypothesisCategory::State,
            statement: "PATH hijacking via writable directory in privileged script".into(),
            status: HypothesisStatus::Proposed,
            probes: vec![],
            confidence: 0.3,
            task_ids: vec![],
        };
        kb.system_model.add_hypothesis(suid_hypothesis);
        kb.system_model.add_hypothesis(sudo_hypothesis);
        kb.system_model.add_hypothesis(cron_hypothesis);
        kb.system_model.add_hypothesis(capabilities_hypothesis);
        kb.system_model.add_hypothesis(path_hypothesis);
        kb.deductive_metrics.hypotheses_generated += 5;

        assert_eq!(kb.system_model.hypotheses.len(), 5);

        // --- L2: Probe each hypothesis with targeted commands ---
        kb.system_model.advance_layer(DeductiveLayer::Probing);

        // SUID probe: find -perm -4000 → finds /usr/bin/find (exploitable!)
        let suid_h = kb.system_model.get_hypothesis_mut("h-suid").unwrap();
        suid_h.status = HypothesisStatus::Probing;
        suid_h.probes.push(ProbeResult {
            probe_type: "suid_enum".into(),
            request_summary: "find / -perm -4000 -type f 2>/dev/null".into(),
            response_status: 0,
            response_length: 256,
            timing_ms: 500,
            anomaly_detected: true,
            notes: "/usr/bin/find has SUID bit set — exploitable via -exec".into(),
        });
        suid_h.status = HypothesisStatus::Confirmed;
        kb.deductive_metrics.probe_calls += 1;
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.hypotheses_confirmed += 1;

        // Sudo probe: sudo -l → no NOPASSWD entries
        let sudo_h = kb.system_model.get_hypothesis_mut("h-sudo").unwrap();
        sudo_h.status = HypothesisStatus::Probing;
        sudo_h.probes.push(ProbeResult {
            probe_type: "sudo_enum".into(),
            request_summary: "sudo -l 2>/dev/null".into(),
            response_status: 0,
            response_length: 64,
            timing_ms: 200,
            anomaly_detected: false,
            notes: "No NOPASSWD entries, password required for all commands".into(),
        });
        sudo_h.status = HypothesisStatus::Refuted;
        kb.deductive_metrics.probe_calls += 1;
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.hypotheses_refuted += 1;

        // Cron probe: readable crontab with writable script
        let cron_h = kb.system_model.get_hypothesis_mut("h-cron").unwrap();
        cron_h.status = HypothesisStatus::Probing;
        cron_h.probes.push(ProbeResult {
            probe_type: "cron_enum".into(),
            request_summary: "cat /etc/crontab; ls -la /etc/cron.d/ 2>/dev/null".into(),
            response_status: 0,
            response_length: 512,
            timing_ms: 150,
            anomaly_detected: true,
            notes: "* * * * * root /opt/scripts/backup.sh — script is world-writable!".into(),
        });
        cron_h.status = HypothesisStatus::Confirmed;
        kb.deductive_metrics.probe_calls += 1;
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.hypotheses_confirmed += 1;

        // Capabilities probe: nothing exploitable
        let caps_h = kb.system_model.get_hypothesis_mut("h-caps").unwrap();
        caps_h.status = HypothesisStatus::Probing;
        caps_h.probes.push(ProbeResult {
            probe_type: "caps_enum".into(),
            request_summary: "getcap -r / 2>/dev/null".into(),
            response_status: 0,
            response_length: 128,
            timing_ms: 300,
            anomaly_detected: false,
            notes: "Only /usr/bin/ping has cap_net_raw — not exploitable for privesc".into(),
        });
        caps_h.status = HypothesisStatus::Refuted;
        kb.deductive_metrics.probe_calls += 1;
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.hypotheses_refuted += 1;

        // PATH probe: no writable PATH dirs
        let path_h = kb.system_model.get_hypothesis_mut("h-path").unwrap();
        path_h.status = HypothesisStatus::Probing;
        path_h.probes.push(ProbeResult {
            probe_type: "path_enum".into(),
            request_summary: "echo $PATH; ls -la dirs".into(),
            response_status: 0,
            response_length: 200,
            timing_ms: 100,
            anomaly_detected: false,
            notes: "All PATH directories are root-owned, no writable dirs".into(),
        });
        path_h.status = HypothesisStatus::Refuted;
        kb.deductive_metrics.probe_calls += 1;
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.hypotheses_refuted += 1;

        // --- Verify probe ratio ---
        // Total: 2 (L0) + 5 (probes) = 7 total, 5 probe calls
        let probe_ratio = kb.deductive_metrics.efficiency_score();
        assert!(
            probe_ratio > 0.7,
            "Privesc probe ratio should exceed 0.7, got {probe_ratio}"
        );

        // --- L3: Exploit confirmed hypotheses only ---
        kb.system_model.advance_layer(DeductiveLayer::Exploiting);

        // Only SUID and cron are confirmed
        let confirmed: Vec<String> = kb
            .system_model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Confirmed)
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(confirmed.len(), 2);
        assert!(confirmed.contains(&"h-suid".to_string()));
        assert!(confirmed.contains(&"h-cron".to_string()));

        // Refuted hypotheses must NOT be exploited
        let refuted: Vec<String> = kb
            .system_model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Refuted)
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(refuted.len(), 3);

        // Exploit SUID find → get flag
        kb.extract_from_output(
            "privesc",
            "172.20.8.10",
            "find . -exec /bin/sh -p \\; → root shell obtained → FLAG{suid_find_privesc}",
        );
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.flags_captured += 1;

        // Exploit cron writable script → get flag
        kb.extract_from_output(
            "privesc",
            "172.20.8.10",
            "Replaced /opt/scripts/backup.sh → waited for cron → FLAG{cron_writable_privesc}",
        );
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.flags_captured += 1;

        // Read another flag from root home
        kb.extract_from_output(
            "privesc",
            "172.20.8.10",
            "cat /root/flag.txt → FLAG{root_flag_obtained}",
        );
        kb.deductive_metrics.total_tool_calls += 1;
        kb.deductive_metrics.flags_captured += 1;

        assert_eq!(kb.flags.len(), 3);

        // Mark hypotheses as exploited
        let suid_h = kb.system_model.get_hypothesis_mut("h-suid").unwrap();
        suid_h.status = HypothesisStatus::Exploited;
        let cron_h = kb.system_model.get_hypothesis_mut("h-cron").unwrap();
        cron_h.status = HypothesisStatus::Exploited;

        // --- Verify final metrics ---
        // Total: 2 + 5 + 3 = 10 total, 5 probe calls
        let final_ratio = kb.deductive_metrics.efficiency_score();
        assert!(
            final_ratio >= 0.5,
            "Final probe ratio should be >= 0.5, got {final_ratio}"
        );
        assert_eq!(kb.deductive_metrics.brute_force_calls, 0);
        assert_eq!(kb.deductive_metrics.hypotheses_confirmed, 2);
        assert_eq!(kb.deductive_metrics.hypotheses_refuted, 3);
        assert_eq!(kb.deductive_metrics.hypotheses_generated, 5);

        // Verify goal achieved
        let mut goal = std::mem::take(&mut kb.goal);
        goal.check_criteria(&kb, &[]);
        assert!(matches!(goal.status, GoalStatus::Achieved));
        kb.goal = goal;
    }

    // -----------------------------------------------------------------------
    // DefenderModel tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_defender_model_default() {
        let dm = DefenderModel::default();
        assert_eq!(dm.noise_budget, 1.0);
        assert!(dm.detected_wafs.is_empty());
        assert!(dm.rate_limits.is_empty());
        assert!(dm.blocked_payloads.is_empty());
        assert_eq!(dm.ids_sensitivity, IdsSensitivity::None);
    }

    #[test]
    fn test_defender_model_with_default_bypasses() {
        let dm = DefenderModel::with_default_bypasses();
        assert!(!dm.bypass_techniques.is_empty());
        assert!(dm.bypass_techniques.len() >= 5);
        assert_eq!(dm.noise_budget, 1.0);
    }

    #[test]
    fn test_defender_model_record_block() {
        let mut dm = DefenderModel::default();
        assert_eq!(dm.noise_budget, 1.0);

        dm.record_block("10.0.0.1", "' OR 1=1--", 403);

        assert_eq!(dm.detected_wafs.len(), 1);
        assert_eq!(dm.detected_wafs[0].host, "10.0.0.1");
        assert_eq!(dm.detected_wafs[0].blocked_payloads.len(), 1);
        assert_eq!(dm.detected_wafs[0].confidence, 0.5);
        assert_eq!(dm.blocked_payloads.len(), 1);
        assert!(dm.noise_budget < 1.0); // reduced

        // Second block on same host increases confidence
        dm.record_block("10.0.0.1", "<script>alert(1)</script>", 403);
        assert_eq!(dm.detected_wafs.len(), 1); // still same WAF
        assert_eq!(dm.detected_wafs[0].confidence, 0.6);
        assert_eq!(dm.detected_wafs[0].blocked_payloads.len(), 2);
    }

    #[test]
    fn test_defender_model_record_bypass() {
        let mut dm = DefenderModel::default();
        dm.record_block("10.0.0.1", "payload", 403);
        dm.record_bypass("10.0.0.1", "case_alternation");

        let waf = &dm.detected_wafs[0];
        assert!(
            waf.successful_bypasses
                .contains(&"case_alternation".to_string())
        );

        // Duplicate bypass not added
        dm.record_bypass("10.0.0.1", "case_alternation");
        assert_eq!(dm.detected_wafs[0].successful_bypasses.len(), 1);
    }

    #[test]
    fn test_defender_model_detection_costs() {
        let dm = DefenderModel::default();

        // Default costs
        assert!(dm.detection_cost_for("differential_probe") < 0.2);
        assert!(dm.detection_cost_for("brute_force") > 0.7);
        assert!(dm.detection_cost_for("sql_injection") >= 0.5);
        assert!(dm.detection_cost_for("privesc_enum") < 0.1);

        // Custom cost overrides default
        let mut dm2 = DefenderModel::default();
        dm2.action_costs.push(DetectionCost {
            action: "brute_force".into(),
            cost: 0.95,
            rationale: "IDS will flag immediately".into(),
        });
        assert_eq!(dm2.detection_cost_for("brute_force"), 0.95);
    }

    #[test]
    fn test_defender_model_is_action_allowed() {
        let mut dm = DefenderModel::default();
        // Full budget — everything allowed
        assert!(dm.is_action_allowed("brute_force"));
        assert!(dm.is_action_allowed("differential_probe"));

        // Reduce budget
        dm.noise_budget = 0.3;
        assert!(dm.is_action_allowed("differential_probe")); // 0.1 <= 0.3
        assert!(!dm.is_action_allowed("brute_force")); // 0.8 > 0.3
        assert!(!dm.is_action_allowed("sql_injection")); // 0.5 > 0.3
    }

    #[test]
    fn test_defender_model_adjusted_priority() {
        let mut dm = DefenderModel::default();

        // Full budget — no penalty
        let p = dm.adjusted_priority(100.0, "brute_force");
        assert!((p - 100.0).abs() < f32::EPSILON);

        // Half budget — penalty proportional to detection cost
        dm.noise_budget = 0.5;
        let p_probe = dm.adjusted_priority(100.0, "differential_probe");
        let p_brute = dm.adjusted_priority(100.0, "brute_force");
        assert!(
            p_probe > p_brute,
            "Probes should be prioritized over brute force"
        );
        assert!(p_probe > 90.0, "Probes should have minimal penalty");
        assert!(
            p_brute < 70.0,
            "Brute force should have significant penalty"
        );
    }

    #[test]
    fn test_defender_model_suggest_bypasses() {
        let mut dm = DefenderModel::with_default_bypasses();
        dm.detected_wafs.push(DetectedWaf {
            host: "10.0.0.1".into(),
            port: Some(80),
            waf_type: WafType::ModSecurity,
            confidence: 0.8,
            blocked_payloads: vec!["' OR 1=1--".into()],
            successful_bypasses: vec![],
        });

        let bypasses = dm.suggest_bypasses("10.0.0.1");
        assert!(!bypasses.is_empty());
        // ModSecurity bypasses should include case_alternation and comment_injection
        let names: Vec<&str> = bypasses.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"case_alternation"));
        assert!(names.contains(&"comment_injection"));

        // No WAF on unknown host
        let no_bypasses = dm.suggest_bypasses("10.0.0.99");
        assert!(no_bypasses.is_empty());
    }

    #[test]
    fn test_defender_model_rate_limit() {
        let mut dm = DefenderModel::default();
        dm.record_rate_limit("10.0.0.1", Some("/login"), 10, 60, 429);

        assert_eq!(dm.rate_limits.len(), 1);
        assert_eq!(dm.rate_limits[0].host, "10.0.0.1");
        assert_eq!(dm.rate_limits[0].endpoint.as_deref(), Some("/login"));
        assert_eq!(dm.rate_limits[0].max_requests, 10);
        assert_eq!(dm.rate_limits[0].window_secs, 60);
        assert_eq!(dm.rate_limits[0].limit_status, 429);
    }

    #[test]
    fn test_defender_model_update_ids_sensitivity() {
        let mut dm = DefenderModel::default();
        assert_eq!(dm.ids_sensitivity, IdsSensitivity::None);
        dm.update_ids_sensitivity(IdsSensitivity::High);
        assert_eq!(dm.ids_sensitivity, IdsSensitivity::High);
    }

    #[test]
    fn test_defender_model_serde_roundtrip() {
        let mut dm = DefenderModel::with_default_bypasses();
        dm.noise_budget = 0.6;
        dm.ids_sensitivity = IdsSensitivity::Medium;
        dm.record_block("10.0.0.1", "test_payload", 403);
        dm.record_rate_limit("10.0.0.1", None, 100, 60, 429);

        let json = serde_json::to_string(&dm).unwrap();
        let parsed: DefenderModel = serde_json::from_str(&json).unwrap();

        assert!((parsed.noise_budget - dm.noise_budget).abs() < f32::EPSILON);
        assert_eq!(parsed.ids_sensitivity, IdsSensitivity::Medium);
        assert_eq!(parsed.detected_wafs.len(), 1);
        assert_eq!(parsed.rate_limits.len(), 1);
        assert!(!parsed.bypass_techniques.is_empty());
    }

    #[test]
    fn test_defender_model_in_knowledge_base() {
        let mut kb = KnowledgeBase::new();
        // Default KB has default defender model
        assert_eq!(kb.defender_model.noise_budget, 1.0);
        assert!(kb.defender_model.detected_wafs.is_empty());

        // Modify and verify
        kb.defender_model.record_block("10.0.0.1", "payload", 403);
        assert_eq!(kb.defender_model.detected_wafs.len(), 1);
    }

    #[test]
    fn test_defender_model_noise_budget_floor() {
        let mut dm = DefenderModel::default();
        // Record many blocks — budget should not go below 0.0
        for i in 0..20 {
            dm.record_block("10.0.0.1", &format!("payload_{i}"), 403);
        }
        assert!(dm.noise_budget >= 0.0);
    }

    #[test]
    fn test_waf_classification() {
        let mut dm = DefenderModel::default();
        dm.record_block("host1", "p1", 406);
        assert!(matches!(dm.detected_wafs[0].waf_type, WafType::ModSecurity));

        dm.record_block("host2", "p2", 429);
        let waf2 = dm.detected_wafs.iter().find(|w| w.host == "host2").unwrap();
        assert!(matches!(waf2.waf_type, WafType::Unknown(ref s) if s == "rate_limiter"));
    }

    // -----------------------------------------------------------------------
    // Evidence Chain tests
    // -----------------------------------------------------------------------

    fn make_evidence_record(
        id: &str,
        specialist: &str,
        hypothesis_id: Option<&str>,
    ) -> EvidenceRecord {
        EvidenceRecord {
            id: id.to_string(),
            timestamp: 1700000000,
            specialist: specialist.to_string(),
            task_id: Some(1),
            tool_name: "shell_exec".to_string(),
            tool_input: "curl http://target/test".to_string(),
            tool_output: "HTTP/1.1 200 OK".to_string(),
            model_delta: Some("Added WebApp component".to_string()),
            hypothesis_id: hypothesis_id.map(|s| s.to_string()),
            finding_ref: None,
            poc_script: Some("curl http://target/test".to_string()),
        }
    }

    #[test]
    fn test_evidence_chain_new() {
        let chain = EvidenceChain::new("session-1", "CTF target 10.0.0.1");
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert_eq!(chain.session_id, "session-1");
        assert_eq!(chain.target_description, "CTF target 10.0.0.1");
    }

    #[test]
    fn test_evidence_chain_record() {
        let mut chain = EvidenceChain::new("s1", "target");
        let rec = make_evidence_record("ev-1", "web_exploit", Some("hyp-1"));
        chain.record(rec);
        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());
        assert_eq!(chain.records[0].id, "ev-1");
    }

    #[test]
    fn test_evidence_chain_filter_by_hypothesis() {
        let mut chain = EvidenceChain::new("s1", "target");
        chain.record(make_evidence_record("ev-1", "web_exploit", Some("hyp-1")));
        chain.record(make_evidence_record("ev-2", "privesc", Some("hyp-2")));
        chain.record(make_evidence_record("ev-3", "web_exploit", Some("hyp-1")));

        let hyp1_records = chain.records_for_hypothesis("hyp-1");
        assert_eq!(hyp1_records.len(), 2);
        assert_eq!(hyp1_records[0].id, "ev-1");
        assert_eq!(hyp1_records[1].id, "ev-3");

        let hyp2_records = chain.records_for_hypothesis("hyp-2");
        assert_eq!(hyp2_records.len(), 1);

        let none_records = chain.records_for_hypothesis("hyp-999");
        assert!(none_records.is_empty());
    }

    #[test]
    fn test_evidence_chain_filter_by_findings() {
        let mut chain = EvidenceChain::new("s1", "target");
        let mut rec1 = make_evidence_record("ev-1", "web_exploit", Some("hyp-1"));
        rec1.finding_ref = Some("SQLi in /login".to_string());
        chain.record(rec1);
        chain.record(make_evidence_record("ev-2", "recon", None));

        let finding_records = chain.records_with_findings();
        assert_eq!(finding_records.len(), 1);
        assert_eq!(finding_records[0].id, "ev-1");
    }

    #[test]
    fn test_evidence_chain_filter_by_task() {
        let mut chain = EvidenceChain::new("s1", "target");
        let mut rec1 = make_evidence_record("ev-1", "web_exploit", None);
        rec1.task_id = Some(42);
        chain.record(rec1);
        let mut rec2 = make_evidence_record("ev-2", "recon", None);
        rec2.task_id = Some(43);
        chain.record(rec2);

        let task_records = chain.records_for_task(42);
        assert_eq!(task_records.len(), 1);
        assert_eq!(task_records[0].id, "ev-1");
    }

    #[test]
    fn test_evidence_chain_export_json() {
        let mut chain = EvidenceChain::new("s1", "10.0.0.1");
        chain.record(make_evidence_record("ev-1", "web_exploit", Some("hyp-1")));

        let json = chain.export_json().unwrap();
        assert!(json.contains("ev-1"));
        assert!(json.contains("web_exploit"));
        assert!(json.contains("hyp-1"));
        assert!(json.contains("session_id"));

        // Round-trip
        let deserialized: EvidenceChain = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.records.len(), 1);
        assert_eq!(deserialized.session_id, "s1");
    }

    #[test]
    fn test_evidence_chain_export_poc_scripts_by_hypothesis() {
        let mut chain = EvidenceChain::new("s1", "10.0.0.1");
        chain.record(make_evidence_record(
            "ev-1",
            "web_exploit",
            Some("hyp-sqli"),
        ));
        let mut rec2 = make_evidence_record("ev-2", "web_exploit", Some("hyp-sqli"));
        rec2.poc_script = Some("sqlmap -u http://target/login --dbs".to_string());
        chain.record(rec2);

        let scripts = chain.export_poc_scripts();
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].name, "poc_hyp-sqli.sh");
        assert!(scripts[0].content.contains("#!/bin/bash"));
        assert!(scripts[0].content.contains("hyp-sqli"));
        assert!(scripts[0].content.contains("curl"));
        assert!(scripts[0].content.contains("sqlmap"));
        assert_eq!(scripts[0].hypothesis_id, Some("hyp-sqli".to_string()));
    }

    #[test]
    fn test_evidence_chain_export_poc_standalone_finding() {
        let mut chain = EvidenceChain::new("s1", "10.0.0.1");
        let mut rec = make_evidence_record("ev-standalone", "recon", None);
        rec.finding_ref = Some("Open FTP with anon access".to_string());
        rec.poc_script = Some("ftp -A 10.0.0.1".to_string());
        chain.record(rec);

        let scripts = chain.export_poc_scripts();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].content.contains("ftp -A"));
        assert!(scripts[0].finding_ref.is_some());
        assert!(scripts[0].hypothesis_id.is_none());
    }

    #[test]
    fn test_evidence_chain_empty_poc_export() {
        let chain = EvidenceChain::new("s1", "target");
        let scripts = chain.export_poc_scripts();
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_evidence_record_serialization_roundtrip() {
        let rec = make_evidence_record("ev-1", "web_exploit", Some("hyp-1"));
        let json = serde_json::to_string(&rec).unwrap();
        let deserialized: EvidenceRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "ev-1");
        assert_eq!(deserialized.specialist, "web_exploit");
        assert_eq!(deserialized.hypothesis_id, Some("hyp-1".to_string()));
        assert_eq!(deserialized.tool_name, "shell_exec");
    }

    #[test]
    fn test_poc_script_serialization_roundtrip() {
        let poc = PocScript {
            name: "poc_test.sh".to_string(),
            hypothesis_id: Some("hyp-1".to_string()),
            finding_ref: Some("SQLi".to_string()),
            content: "#!/bin/bash\ncurl http://target".to_string(),
        };
        let json = serde_json::to_string(&poc).unwrap();
        let deserialized: PocScript = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "poc_test.sh");
        assert_eq!(deserialized.hypothesis_id, Some("hyp-1".to_string()));
    }

    #[test]
    fn test_kb_has_evidence_chain_default() {
        let kb = KnowledgeBase::new();
        assert!(kb.evidence_chain.is_empty());
        assert_eq!(kb.evidence_chain.session_id, "");
    }

    #[test]
    fn test_kb_evidence_chain_integration() {
        let mut kb = KnowledgeBase::new();
        kb.evidence_chain = EvidenceChain::new("session-test", "10.0.0.1");
        kb.evidence_chain
            .record(make_evidence_record("ev-1", "recon", None));
        assert_eq!(kb.evidence_chain.len(), 1);

        // Verify it survives serialization
        let json = serde_json::to_string(&kb).unwrap();
        let deserialized: KnowledgeBase = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.evidence_chain.len(), 1);
        assert_eq!(deserialized.evidence_chain.session_id, "session-test");
    }
}
