use crate::core::analysis::AnalysisResult;
use crate::core::enrich::EnrichmentWorker;
use crate::error::Error;

pub struct ClaudeEnrichmentWorker;

inventory::submit!(&ClaudeEnrichmentWorker as &dyn EnrichmentWorker);

impl EnrichmentWorker for ClaudeEnrichmentWorker {
    fn agent_name(&self) -> &str {
        "claude_code"
    }

    fn enrich(&self, _result: &mut AnalysisResult) -> Result<(), Error> {
        // TODO: Read ~/.claude/sessions/ to correlate agent_session_id
        // TODO: Read task lists, loaded skills, model info
        // This is best-effort -- if .claude/ format changes, we degrade gracefully
        Ok(())
    }
}
