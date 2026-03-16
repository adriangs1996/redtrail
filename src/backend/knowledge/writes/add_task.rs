use super::super::KnowledgeBase;
use super::super::types::task::{TaskStatus, TaskSummary};

impl KnowledgeBase {
    pub fn add_task_summary(&mut self, summary: TaskSummary) {
        match summary.status {
            TaskStatus::Completed => self.completed_tasks.push(summary),
            TaskStatus::Failed | TaskStatus::Timeout => self.failed_tasks.push(summary),
        }
    }
}
