use crate::agent::tools::{ToolDef, ToolRegistry};
use std::collections::HashSet;

pub fn filter_tools(registry: &ToolRegistry, enabled: &HashSet<String>) -> Vec<ToolDef> {
    registry
        .definitions()
        .into_iter()
        .filter(|t| enabled.contains(&t.name))
        .cloned()
        .collect()
}
