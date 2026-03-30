pub mod claude;

use crate::core::analysis::AnalysisResult;
use crate::error::Error;

/// Trait for agent-specific enrichment workers.
pub trait EnrichmentWorker: Send + Sync {
    /// The agent source name this worker handles (e.g., "claude_code").
    fn agent_name(&self) -> &str;

    /// Attempt to enrich the analysis result with agent-specific data.
    /// Must never panic. Returns Ok(()) even if no enrichment was possible.
    fn enrich(&self, result: &mut AnalysisResult) -> Result<(), Error>;
}

inventory::collect!(&'static dyn EnrichmentWorker);

/// Run all matching enrichment workers for the given agent source.
/// Best-effort: failures are silently ignored.
pub fn run_enrichment(source: &str, result: &mut AnalysisResult) {
    for worker in inventory::iter::<&dyn EnrichmentWorker> {
        if worker.agent_name() == source {
            let _ = worker.enrich(result);
        }
    }
}
