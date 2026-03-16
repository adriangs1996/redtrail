use super::super::KnowledgeBase;
use super::super::SpecialistRun;

impl KnowledgeBase {
    pub fn record_specialist_run(&mut self, specialist_name: &str) {
        let count = self.total_findings_count();
        if !self
            .activated_specialists
            .contains(&specialist_name.to_string())
        {
            self.activated_specialists.push(specialist_name.to_string());
        }
        if let Some(run) = self
            .specialist_runs
            .iter_mut()
            .find(|r| r.name == specialist_name)
        {
            run.findings_count_at_run = count;
        } else {
            self.specialist_runs.push(SpecialistRun {
                name: specialist_name.to_string(),
                findings_count_at_run: count,
            });
        }
    }

    pub fn record_custom_definition(&mut self, name: &str) {
        if !self.custom_definitions_used.contains(&name.to_string()) {
            self.custom_definitions_used.push(name.to_string());
        }
    }
}
