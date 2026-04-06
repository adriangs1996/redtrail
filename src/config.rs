use crate::error::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
    #[serde(default)]
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_llm_provider")]
    pub provider: String,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default = "default_llm_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_input_chars")]
    pub max_input_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub url: String,
    #[serde(default = "default_ollama_model")]
    pub model: String,
}

fn default_llm_provider() -> String {
    "ollama".to_string()
}
fn default_llm_timeout() -> u64 {
    30
}
fn default_max_input_chars() -> usize {
    4096
}
fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}
fn default_ollama_model() -> String {
    "gemma4".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_llm_provider(),
            ollama: OllamaConfig::default(),
            timeout_seconds: default_llm_timeout(),
            max_input_chars: default_max_input_chars(),
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: default_ollama_url(),
            model: default_ollama_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_blacklist")]
    pub blacklist_commands: Vec<String>,
    #[serde(default = "default_max_stdout")]
    pub max_stdout_bytes: usize,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum OnDetect {
    #[default]
    Redact,
    Warn,
    Block,
}

impl std::fmt::Display for OnDetect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OnDetect::Redact => write!(f, "redact"),
            OnDetect::Warn => write!(f, "warn"),
            OnDetect::Block => write!(f, "block"),
        }
    }
}

impl std::str::FromStr for OnDetect {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "redact" => Ok(OnDetect::Redact),
            "warn" => Ok(OnDetect::Warn),
            "block" => Ok(OnDetect::Block),
            other => Err(format!(
                "invalid on_detect value: {other} (expected redact, warn, or block)"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default = "default_true")]
    pub redact: bool,
    #[serde(default)]
    pub on_detect: OnDetect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patterns_file: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_max_stdout() -> usize {
    51200
}
fn default_retention_days() -> u32 {
    90
}
fn default_blacklist() -> Vec<String> {
    vec![
        "vim", "nvim", "nano", "vi", "ssh", "scp", "top", "htop", "btop", "less", "more", "man",
        "tmux", "screen",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            blacklist_commands: default_blacklist(),
            max_stdout_bytes: default_max_stdout(),
            retention_days: default_retention_days(),
        }
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            redact: true,
            on_detect: OnDetect::default(),
            patterns_file: None,
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Error> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_yaml::from_str(&contents)
                .map_err(|e| Error::Config(format!("invalid config: {e}"))),
            Err(_) => Ok(Config::default()),
        }
    }

    pub fn save(&self, path: &str) -> Result<(), Error> {
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| Error::Config(format!("serialize error: {e}")))?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, yaml)?;
        Ok(())
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<(), Error> {
        match key {
            "capture.enabled" => {
                self.capture.enabled = value
                    .parse()
                    .map_err(|_| Error::Config("expected bool".into()))?
            }
            "capture.max_stdout_bytes" => {
                self.capture.max_stdout_bytes = value
                    .parse()
                    .map_err(|_| Error::Config("expected number".into()))?
            }
            "capture.retention_days" => {
                self.capture.retention_days = value
                    .parse()
                    .map_err(|_| Error::Config("expected number".into()))?
            }
            "secrets.redact" => {
                self.secrets.redact = value
                    .parse()
                    .map_err(|_| Error::Config("expected bool".into()))?
            }
            "secrets.on_detect" => {
                self.secrets.on_detect = value.parse::<OnDetect>().map_err(Error::Config)?
            }
            "secrets.patterns_file" => {
                self.secrets.patterns_file = Some(value.to_string());
            }
            "llm.enabled" => {
                self.llm.enabled = value
                    .parse()
                    .map_err(|_| Error::Config("expected bool".into()))?
            }
            "llm.provider" => {
                self.llm.provider = value.to_string();
            }
            "llm.ollama.url" => {
                self.llm.ollama.url = value.to_string();
            }
            "llm.ollama.model" => {
                self.llm.ollama.model = value.to_string();
            }
            "llm.timeout_seconds" => {
                self.llm.timeout_seconds = value
                    .parse()
                    .map_err(|_| Error::Config("expected number".into()))?
            }
            "llm.max_input_chars" => {
                self.llm.max_input_chars = value
                    .parse()
                    .map_err(|_| Error::Config("expected number".into()))?
            }
            _ => return Err(Error::Config(format!("unknown config key: {key}"))),
        }
        Ok(())
    }
}
