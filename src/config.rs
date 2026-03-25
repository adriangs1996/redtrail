use serde::{Deserialize, Serialize};

fn default_llm_provider() -> String { "claude-code".to_string() }
fn default_llm_model() -> String { "claude-sonnet-4-20250514".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { llm_provider: default_llm_provider(), llm_model: default_llm_model() }
    }
}
