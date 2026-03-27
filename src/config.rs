use crate::error::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default = "default_true")]
    pub redact: bool,
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
        Self { redact: true }
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
            _ => return Err(Error::Config(format!("unknown config key: {key}"))),
        }
        Ok(())
    }
}
