use serde::{Deserialize, Serialize};

use super::super::KnowledgeBase;
use crate::types::{Finding, Severity};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CriterionCheck {
    FlagsCaptured {
        min_count: u32,
    },
    AccessObtained {
        host: String,
        min_privilege: String,
    },
    DataExtracted {
        description: String,
    },
    VulnsFound {
        min_count: u32,
        min_severity: String,
    },
    Custom {
        description: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    pub description: String,
    pub check: CriterionCheck,
    pub met: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum AssessmentDepth {
    #[default]
    Quick,
    Standard,
    Deep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoalType {
    CaptureFlags {
        flag_pattern: String,
        expected_count: Option<u32>,
    },
    GainAccess {
        target_host: String,
        privilege_level: String,
    },
    Exfiltrate {
        target: String,
    },
    VulnerabilityAssessment {
        scope: Vec<String>,
        depth: AssessmentDepth,
    },
    Custom {
        objective: String,
    },
}

impl Default for GoalType {
    fn default() -> Self {
        GoalType::Custom {
            objective: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GoalStatus {
    #[default]
    InProgress,
    Achieved,
    PartiallyAchieved,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionGoal {
    pub goal_type: GoalType,
    pub description: String,
    pub success_criteria: Vec<Criterion>,
    pub status: GoalStatus,
}

impl Severity {
    fn rank(&self) -> u8 {
        match self {
            Severity::Critical => 4,
            Severity::High => 3,
            Severity::Medium => 2,
            Severity::Low => 1,
            Severity::Info => 0,
        }
    }

    fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" => Severity::Critical,
            "high" => Severity::High,
            "medium" => Severity::Medium,
            "low" => Severity::Low,
            _ => Severity::Info,
        }
    }
}

impl SessionGoal {
    pub fn check_criteria(&mut self, kb: &KnowledgeBase, findings: &[Finding]) {
        for criterion in &mut self.success_criteria {
            match &criterion.check {
                CriterionCheck::FlagsCaptured { min_count } => {
                    criterion.met = kb.flags.len() as u32 >= *min_count;
                }
                CriterionCheck::AccessObtained {
                    host,
                    min_privilege,
                } => {
                    let required_rank = Severity::from_str_loose(min_privilege).rank();
                    criterion.met = kb.access_levels.iter().any(|a| {
                        a.host == *host
                            && Severity::from_str_loose(&a.privilege_level).rank() >= required_rank
                    });
                }
                CriterionCheck::VulnsFound {
                    min_count,
                    min_severity,
                } => {
                    let threshold = Severity::from_str_loose(min_severity).rank();
                    let count = findings
                        .iter()
                        .filter(|f| f.severity.rank() >= threshold)
                        .count() as u32;
                    criterion.met = count >= *min_count;
                }
                CriterionCheck::DataExtracted { .. } | CriterionCheck::Custom { .. } => {}
            }
        }

        let total = self.success_criteria.len();
        let met_count = self.success_criteria.iter().filter(|c| c.met).count();

        self.status = if total == 0 || met_count == 0 {
            GoalStatus::InProgress
        } else if met_count == total {
            GoalStatus::Achieved
        } else {
            GoalStatus::PartiallyAchieved
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSession {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub target_url: Option<String>,
    pub target_hosts: Vec<String>,
    pub total_turns_used: u32,
    pub max_turns_configured: u32,
    pub llm_provider: String,
    pub knowledge: KnowledgeBase,
    pub findings: Vec<Finding>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Completed,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: String,
    pub target_url: Option<String>,
    pub total_turns_used: u32,
    pub findings_count: usize,
    pub status: SessionStatus,
}
