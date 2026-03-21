use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::path::Path;

fn default_autonomy() -> String {
    "balanced".to_string()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_llm_provider() -> String {
    "anthropic".to_string()
}
fn default_llm_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_noise_threshold() -> u8 {
    5
}
fn default_flag_patterns() -> Vec<String> {
    vec![
        r"HTB\{[^}]+\}".to_string(),
        r"FLAG\{[^}]+\}".to_string(),
        r"flag\{[^}]+\}".to_string(),
    ]
}
fn default_tool_aliases() -> Vec<String> {
    vec![
        "nmap",
        "gobuster",
        "feroxbuster",
        "ffuf",
        "dirb",
        "nikto",
        "sqlmap",
        "hydra",
        "crackmapexec",
        "whatweb",
        "nuclei",
        "john",
        "hashcat",
        "curl",
        "wget",
        "ssh",
        "scp",
        "nc",
        "netcat",
        "enum4linux",
        "responder",
        "wfuzz",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
fn default_max_sessions() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_autonomy")]
    pub autonomy: String,
    #[serde(default = "default_true")]
    pub auto_extract: bool,
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            autonomy: default_autonomy(),
            auto_extract: default_true(),
            llm_provider: default_llm_provider(),
            llm_model: default_llm_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScopeConfig {
    #[serde(default = "default_false")]
    pub strict: bool,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseConfig {
    #[serde(default = "default_noise_threshold")]
    pub threshold: u8,
    #[serde(default = "default_true")]
    pub filter_duplicates: bool,
}

impl Default for NoiseConfig {
    fn default() -> Self {
        Self {
            threshold: default_noise_threshold(),
            filter_duplicates: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagConfig {
    #[serde(default = "default_flag_patterns")]
    pub patterns: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_capture: bool,
}

impl Default for FlagConfig {
    fn default() -> Self {
        Self {
            patterns: default_flag_patterns(),
            auto_capture: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_tool_aliases")]
    pub aliases: Vec<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            aliases: default_tool_aliases(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_max_sessions")]
    pub max_sessions: u32,
    #[serde(default = "default_true")]
    pub auto_save: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_sessions: default_max_sessions(),
            auto_save: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub scope: ScopeConfig,
    #[serde(default)]
    pub noise: NoiseConfig,
    #[serde(default)]
    pub flags: FlagConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

impl Config {
    pub fn load_global() -> Result<Self, Error> {
        let path = dirs::home_dir()
            .ok_or_else(|| Error::Config("cannot determine home directory".to_string()))?
            .join(".redtrail/config.toml");

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(|e| Error::Config(e.to_string()))
    }

    pub fn load_workspace(workspace_path: &Path) -> Result<Option<String>, Error> {
        let path = workspace_path.join(".redtrail/config.toml");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(&path)?))
    }

    pub fn merge_workspace(mut self, ws_toml: &str) -> Result<Self, Error> {
        let ws_val: toml::Value =
            toml::from_str(ws_toml).map_err(|e| Error::Config(e.to_string()))?;

        let ws_table = match &ws_val {
            toml::Value::Table(t) => t,
            _ => {
                return Err(Error::Config(
                    "workspace config must be a TOML table".to_string(),
                ));
            }
        };

        if let Some(toml::Value::Table(g)) = ws_table.get("general") {
            if let Some(v) = g.get("autonomy")
                && let Some(s) = v.as_str()
            {
                self.general.autonomy = s.to_string();
            }
            if let Some(v) = g.get("auto_extract")
                && let Some(b) = v.as_bool()
            {
                self.general.auto_extract = b;
            }
            if let Some(v) = g.get("llm_provider")
                && let Some(s) = v.as_str()
            {
                self.general.llm_provider = s.to_string();
            }
            if let Some(v) = g.get("llm_model")
                && let Some(s) = v.as_str()
            {
                self.general.llm_model = s.to_string();
            }
        }

        if let Some(toml::Value::Table(s)) = ws_table.get("scope") {
            if let Some(v) = s.get("strict")
                && let Some(b) = v.as_bool()
            {
                self.scope.strict = b;
            }
            if let Some(toml::Value::Array(hosts)) = s.get("allowed_hosts") {
                self.scope.allowed_hosts = hosts
                    .iter()
                    .filter_map(|h| h.as_str().map(String::from))
                    .collect();
            }
        }

        if let Some(toml::Value::Table(n)) = ws_table.get("noise") {
            if let Some(v) = n.get("threshold")
                && let Some(i) = v.as_integer()
            {
                self.noise.threshold = i as u8;
            }
            if let Some(v) = n.get("filter_duplicates")
                && let Some(b) = v.as_bool()
            {
                self.noise.filter_duplicates = b;
            }
        }

        if let Some(toml::Value::Table(f)) = ws_table.get("flags") {
            if let Some(toml::Value::Array(patterns)) = f.get("patterns") {
                self.flags.patterns = patterns
                    .iter()
                    .filter_map(|p| p.as_str().map(String::from))
                    .collect();
            }
            if let Some(v) = f.get("auto_capture")
                && let Some(b) = v.as_bool()
            {
                self.flags.auto_capture = b;
            }
        }

        if let Some(toml::Value::Table(t)) = ws_table.get("tools")
            && let Some(toml::Value::Array(aliases)) = t.get("aliases")
        {
            self.tools.aliases = aliases
                .iter()
                .filter_map(|a| a.as_str().map(String::from))
                .collect();
        }

        if let Some(toml::Value::Table(s)) = ws_table.get("session") {
            if let Some(v) = s.get("max_sessions")
                && let Some(i) = v.as_integer()
            {
                self.session.max_sessions = i as u32;
            }
            if let Some(v) = s.get("auto_save")
                && let Some(b) = v.as_bool()
            {
                self.session.auto_save = b;
            }
        }

        Ok(self)
    }

    pub fn resolved(workspace_dir: &Path) -> Result<Self, Error> {
        let global = Self::load_global()?;
        match Self::load_workspace(workspace_dir)? {
            Some(ws_toml) => global.merge_workspace(&ws_toml),
            None => Ok(global),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.general.autonomy, "balanced");
        assert!(cfg.tools.aliases.contains(&"nmap".to_string()));
    }

    #[test]
    fn test_merge_workspace_overrides_tools() {
        let global = Config::default();
        let ws_toml = r#"
[tools]
aliases = ["nmap", "curl"]
"#;
        let merged = global.merge_workspace(ws_toml).unwrap();
        assert_eq!(merged.tools.aliases, vec!["nmap", "curl"]);
    }

    #[test]
    fn test_merge_workspace_preserves_unset_fields() {
        let global = Config::default();
        let ws_toml = r#"
[scope]
strict = true
"#;
        let merged = global.merge_workspace(ws_toml).unwrap();
        assert!(merged.scope.strict);
        assert_eq!(merged.general.autonomy, "balanced");
    }

    #[test]
    fn test_merge_can_override_back_to_default() {
        let mut global = Config::default();
        global.general.autonomy = "autonomous".to_string();
        let ws_toml = r#"
[general]
autonomy = "balanced"
"#;
        let merged = global.merge_workspace(ws_toml).unwrap();
        assert_eq!(merged.general.autonomy, "balanced");
    }

    #[test]
    fn test_default_llm_provider() {
        let cfg = Config::default();
        assert_eq!(cfg.general.llm_provider, "anthropic");
        assert_eq!(cfg.general.llm_model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_merge_workspace_overrides_llm_fields() {
        let global = Config::default();
        let ws_toml = r#"
[general]
llm_provider = "ollama"
llm_model = "llama3"
"#;
        let merged = global.merge_workspace(ws_toml).unwrap();
        assert_eq!(merged.general.llm_provider, "ollama");
        assert_eq!(merged.general.llm_model, "llama3");
    }
}
