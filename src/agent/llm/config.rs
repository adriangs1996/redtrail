use super::LlmError;
use super::LlmProvider;
use super::anthropic_api::{AnthropicApiConfig, AnthropicApiProvider};
use super::ollama::{OllamaConfig, OllamaProvider};

pub enum LlmConfig {
    AnthropicApi(AnthropicApiConfig),
    Ollama(OllamaConfig),
}

pub fn create_provider(config: LlmConfig) -> Result<Box<dyn LlmProvider>, LlmError> {
    match config {
        LlmConfig::AnthropicApi(cfg) => Ok(Box::new(AnthropicApiProvider::new(cfg)?)),
        LlmConfig::Ollama(cfg) => Ok(Box::new(OllamaProvider::new(cfg)?)),
    }
}
