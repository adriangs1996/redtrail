use super::super::KnowledgeBase;

impl KnowledgeBase {
    pub fn has_new_findings_for(&self, specialist_name: &str) -> bool {
        let total_findings = self.total_findings_count();
        match self
            .specialist_runs
            .iter()
            .find(|r| r.name == specialist_name)
        {
            Some(run) => total_findings > run.findings_count_at_run,
            None => total_findings > 0,
        }
    }
}
