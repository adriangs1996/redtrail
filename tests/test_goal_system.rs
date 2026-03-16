//! # Goal System Tests
//!
//! Redtrail replaces hardcoded FLAG{...} detection with a flexible goal system.
//! Sessions can target: flag capture, access gain, data exfiltration,
//! vulnerability assessment, or custom objectives.
//!
//! These tests validate:
//! 1. Goal criteria checking against KB state
//! 2. Goal status transitions (InProgress → Achieved / PartiallyAchieved)
//! 3. Deterministic completion (goal-driven, not LLM-driven)

use redtrail::agent::knowledge::{
    AccessLevel, Criterion, CriterionCheck, GoalStatus, GoalType, KnowledgeBase, SessionGoal,
};
use redtrail::{Finding, Severity, VulnType};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn capture_flags_goal(expected: u32) -> SessionGoal {
    SessionGoal {
        goal_type: GoalType::CaptureFlags {
            flag_pattern: r"FLAG\{[^}]+\}".into(),
            expected_count: Some(expected),
        },
        description: format!("Capture {expected} flags"),
        success_criteria: vec![Criterion {
            description: format!("At least {expected} flags captured"),
            check: CriterionCheck::FlagsCaptured {
                min_count: expected,
            },
            met: false,
        }],
        status: GoalStatus::InProgress,
    }
}

fn gain_access_goal(host: &str, privilege: &str) -> SessionGoal {
    SessionGoal {
        goal_type: GoalType::GainAccess {
            target_host: host.into(),
            privilege_level: privilege.into(),
        },
        description: format!("Gain {privilege} access on {host}"),
        success_criteria: vec![Criterion {
            description: format!("{privilege} access on {host}"),
            check: CriterionCheck::AccessObtained {
                host: host.into(),
                min_privilege: privilege.into(),
            },
            met: false,
        }],
        status: GoalStatus::InProgress,
    }
}

fn vuln_assessment_goal(min_count: u32, min_severity: &str) -> SessionGoal {
    SessionGoal {
        goal_type: GoalType::VulnerabilityAssessment {
            scope: vec!["http://target".into()],
            depth: Default::default(),
        },
        description: format!("Find at least {min_count} {min_severity}+ vulns"),
        success_criteria: vec![Criterion {
            description: format!("{min_count} {min_severity}+ vulnerabilities"),
            check: CriterionCheck::VulnsFound {
                min_count,
                min_severity: min_severity.into(),
            },
            met: false,
        }],
        status: GoalStatus::InProgress,
    }
}

fn make_finding(vuln: VulnType, severity: Severity) -> Finding {
    Finding {
        vuln_type: vuln,
        severity,
        endpoint: "/test".into(),
        evidence: vec![],
        description: "Test finding".into(),
        fix_suggestion: "Fix it".into(),
    }
}

// ===========================================================================
// CaptureFlags goal
// ===========================================================================

/// Goal not met when no flags captured.
#[test]
fn capture_flags_not_met_empty() {
    let mut goal = capture_flags_goal(3);
    let kb = KnowledgeBase::new();

    goal.check_criteria(&kb, &[]);

    assert!(!goal.success_criteria[0].met);
    assert!(matches!(goal.status, GoalStatus::InProgress));
}

/// Goal partially met — some flags but not enough.
#[test]
fn capture_flags_partial() {
    let mut goal = capture_flags_goal(3);
    let mut kb = KnowledgeBase::new();
    kb.flags.push("FLAG{first}".into());
    kb.flags.push("FLAG{second}".into());

    goal.check_criteria(&kb, &[]);

    assert!(!goal.success_criteria[0].met);
    assert!(matches!(goal.status, GoalStatus::InProgress));
}

/// Goal achieved — enough flags captured.
#[test]
fn capture_flags_achieved() {
    let mut goal = capture_flags_goal(3);
    let mut kb = KnowledgeBase::new();
    kb.flags.push("FLAG{first}".into());
    kb.flags.push("FLAG{second}".into());
    kb.flags.push("FLAG{third}".into());

    goal.check_criteria(&kb, &[]);

    assert!(goal.success_criteria[0].met);
    assert!(matches!(goal.status, GoalStatus::Achieved));
}

/// Goal achieved with MORE flags than expected.
#[test]
fn capture_flags_exceeded() {
    let mut goal = capture_flags_goal(2);
    let mut kb = KnowledgeBase::new();
    kb.flags
        .extend(vec!["FLAG{a}".into(), "FLAG{b}".into(), "FLAG{c}".into()]);

    goal.check_criteria(&kb, &[]);

    assert!(goal.success_criteria[0].met);
    assert!(matches!(goal.status, GoalStatus::Achieved));
}

// ===========================================================================
// GainAccess goal
// ===========================================================================

/// Access not gained yet.
#[test]
fn gain_access_not_met() {
    let mut goal = gain_access_goal("10.0.0.1", "root");
    let kb = KnowledgeBase::new();

    goal.check_criteria(&kb, &[]);

    assert!(!goal.success_criteria[0].met);
    assert!(matches!(goal.status, GoalStatus::InProgress));
}

/// User-level access when root required → not met.
#[test]
fn gain_access_insufficient_privilege() {
    let mut goal = gain_access_goal("10.0.0.1", "high");
    let mut kb = KnowledgeBase::new();
    kb.access_levels.push(AccessLevel {
        host: "10.0.0.1".into(),
        user: "www-data".into(),
        privilege_level: "low".into(),
        method: "ssh".into(),
    });

    goal.check_criteria(&kb, &[]);

    assert!(!goal.success_criteria[0].met);
}

/// Correct host and sufficient privilege → met.
#[test]
fn gain_access_achieved() {
    let mut goal = gain_access_goal("10.0.0.1", "high");
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

/// Wrong host → not met.
#[test]
fn gain_access_wrong_host() {
    let mut goal = gain_access_goal("10.0.0.1", "high");
    let mut kb = KnowledgeBase::new();
    kb.access_levels.push(AccessLevel {
        host: "10.0.0.2".into(),
        user: "root".into(),
        privilege_level: "critical".into(),
        method: "ssh".into(),
    });

    goal.check_criteria(&kb, &[]);

    assert!(!goal.success_criteria[0].met);
}

// ===========================================================================
// VulnerabilityAssessment goal
// ===========================================================================

/// No findings → not met.
#[test]
fn vuln_assessment_no_findings() {
    let mut goal = vuln_assessment_goal(2, "high");
    let kb = KnowledgeBase::new();

    goal.check_criteria(&kb, &[]);

    assert!(!goal.success_criteria[0].met);
}

/// Findings below severity threshold → not counted.
#[test]
fn vuln_assessment_below_severity() {
    let mut goal = vuln_assessment_goal(2, "high");
    let kb = KnowledgeBase::new();
    let findings = vec![
        make_finding(VulnType::InformationDisclosure, Severity::Low),
        make_finding(VulnType::InformationDisclosure, Severity::Medium),
    ];

    goal.check_criteria(&kb, &findings);

    assert!(!goal.success_criteria[0].met);
}

/// Findings at/above severity threshold → counted.
#[test]
fn vuln_assessment_achieved() {
    let mut goal = vuln_assessment_goal(2, "high");
    let kb = KnowledgeBase::new();
    let findings = vec![
        make_finding(VulnType::SqlInjection, Severity::Critical),
        make_finding(VulnType::StoredXSS, Severity::High),
        make_finding(VulnType::InformationDisclosure, Severity::Low), // Not counted
    ];

    goal.check_criteria(&kb, &findings);

    assert!(goal.success_criteria[0].met);
    assert!(matches!(goal.status, GoalStatus::Achieved));
}

// ===========================================================================
// Multi-criteria goals
// ===========================================================================

/// Goal with multiple criteria: all met → Achieved.
#[test]
fn multi_criteria_all_met() {
    let mut goal = SessionGoal {
        goal_type: GoalType::Custom {
            objective: "Full pentest".into(),
        },
        description: "Capture flags and find vulns".into(),
        success_criteria: vec![
            Criterion {
                description: "2 flags".into(),
                check: CriterionCheck::FlagsCaptured { min_count: 2 },
                met: false,
            },
            Criterion {
                description: "1 high vuln".into(),
                check: CriterionCheck::VulnsFound {
                    min_count: 1,
                    min_severity: "high".into(),
                },
                met: false,
            },
        ],
        status: GoalStatus::InProgress,
    };

    let mut kb = KnowledgeBase::new();
    kb.flags.extend(vec!["FLAG{a}".into(), "FLAG{b}".into()]);
    let findings = vec![make_finding(VulnType::SqlInjection, Severity::Critical)];

    goal.check_criteria(&kb, &findings);

    assert!(goal.success_criteria[0].met);
    assert!(goal.success_criteria[1].met);
    assert!(matches!(goal.status, GoalStatus::Achieved));
}

/// Goal with multiple criteria: some met → PartiallyAchieved.
#[test]
fn multi_criteria_partial() {
    let mut goal = SessionGoal {
        goal_type: GoalType::Custom {
            objective: "Full pentest".into(),
        },
        description: "Capture flags and find vulns".into(),
        success_criteria: vec![
            Criterion {
                description: "2 flags".into(),
                check: CriterionCheck::FlagsCaptured { min_count: 2 },
                met: false,
            },
            Criterion {
                description: "1 high vuln".into(),
                check: CriterionCheck::VulnsFound {
                    min_count: 1,
                    min_severity: "high".into(),
                },
                met: false,
            },
        ],
        status: GoalStatus::InProgress,
    };

    let mut kb = KnowledgeBase::new();
    kb.flags.extend(vec!["FLAG{a}".into(), "FLAG{b}".into()]);
    let findings: Vec<Finding> = vec![]; // No findings

    goal.check_criteria(&kb, &findings);

    assert!(goal.success_criteria[0].met); // flags OK
    assert!(!goal.success_criteria[1].met); // vulns not found
    assert!(matches!(goal.status, GoalStatus::PartiallyAchieved));
}

/// Goal with no criteria → stays InProgress.
#[test]
fn no_criteria_stays_in_progress() {
    let mut goal = SessionGoal {
        goal_type: GoalType::Custom {
            objective: "Explore".into(),
        },
        description: "Open-ended exploration".into(),
        success_criteria: vec![],
        status: GoalStatus::InProgress,
    };

    let kb = KnowledgeBase::new();
    goal.check_criteria(&kb, &[]);

    assert!(matches!(goal.status, GoalStatus::InProgress));
}
