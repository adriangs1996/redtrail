use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSummary {
    pub task_name: String,
    pub task_type: String,
    pub target_host: String,
    pub duration_secs: u64,
    pub key_findings: String,
    pub status: TaskStatus,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Completed,
    Failed,
    Timeout,
}
