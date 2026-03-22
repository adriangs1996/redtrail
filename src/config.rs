use crate::error::Error;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

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
    pub fn resolved(conn: &Connection, session_id: &str) -> Result<Self, Error> {
        let mut cfg = Self::default();

        let global = crate::db::config::get_global_config(conn)?;
        for (key, value) in &global {
            cfg.apply_key(key, value);
        }

        let session = crate::db::config::get_session_config(conn, session_id)?;
        for (key, value) in &session {
            cfg.apply_key(key, value);
        }

        Ok(cfg)
    }

    pub fn resolved_global(conn: &Connection) -> Result<Self, Error> {
        let mut cfg = Self::default();
        let global = crate::db::config::get_global_config(conn)?;
        for (key, value) in &global {
            cfg.apply_key(key, value);
        }
        Ok(cfg)
    }

    pub fn apply_key(&mut self, key: &str, value: &str) {
        match key {
            "general.autonomy" => self.general.autonomy = value.to_string(),
            "general.auto_extract" => {
                self.general.auto_extract = value.parse().unwrap_or(self.general.auto_extract)
            }
            "general.llm_provider" => self.general.llm_provider = value.to_string(),
            "general.llm_model" => self.general.llm_model = value.to_string(),
            "noise.threshold" => {
                self.noise.threshold = value.parse().unwrap_or(self.noise.threshold)
            }
            "noise.filter_duplicates" => {
                self.noise.filter_duplicates =
                    value.parse().unwrap_or(self.noise.filter_duplicates)
            }
            "flags.patterns" => {
                if let Ok(v) = serde_json::from_str::<Vec<String>>(value) {
                    self.flags.patterns = v;
                }
            }
            "flags.auto_capture" => {
                self.flags.auto_capture = value.parse().unwrap_or(self.flags.auto_capture)
            }
            "tools.aliases" => {
                if let Ok(v) = serde_json::from_str::<Vec<String>>(value) {
                    self.tools.aliases = v;
                }
            }
            "scope.strict" => self.scope.strict = value.parse().unwrap_or(self.scope.strict),
            "scope.allowed_hosts" => {
                if let Ok(v) = serde_json::from_str::<Vec<String>>(value) {
                    self.scope.allowed_hosts = v;
                }
            }
            "session.max_sessions" => {
                self.session.max_sessions = value.parse().unwrap_or(self.session.max_sessions)
            }
            "session.auto_save" => {
                self.session.auto_save = value.parse().unwrap_or(self.session.auto_save)
            }
            _ => {}
        }
    }

    pub fn get_key(&self, key: &str) -> Option<String> {
        match key {
            "general.autonomy" => Some(self.general.autonomy.clone()),
            "general.auto_extract" => Some(self.general.auto_extract.to_string()),
            "general.llm_provider" => Some(self.general.llm_provider.clone()),
            "general.llm_model" => Some(self.general.llm_model.clone()),
            "noise.threshold" => Some(self.noise.threshold.to_string()),
            "noise.filter_duplicates" => Some(self.noise.filter_duplicates.to_string()),
            "flags.patterns" => serde_json::to_string(&self.flags.patterns).ok(),
            "flags.auto_capture" => Some(self.flags.auto_capture.to_string()),
            "tools.aliases" => serde_json::to_string(&self.tools.aliases).ok(),
            "scope.strict" => Some(self.scope.strict.to_string()),
            "scope.allowed_hosts" => serde_json::to_string(&self.scope.allowed_hosts).ok(),
            "session.max_sessions" => Some(self.session.max_sessions.to_string()),
            "session.auto_save" => Some(self.session.auto_save.to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    fn insert_session(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, "test", "/tmp/test", "", "", "general"],
        ).unwrap();
    }

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.general.autonomy, "balanced");
        assert!(cfg.tools.aliases.contains(&"nmap".to_string()));
    }

    #[test]
    fn test_default_llm_provider() {
        let cfg = Config::default();
        assert_eq!(cfg.general.llm_provider, "anthropic");
        assert_eq!(cfg.general.llm_model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_resolved_defaults_only() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert_eq!(cfg.general.autonomy, "balanced");
        assert_eq!(cfg.noise.threshold, 5);
        assert!(cfg.general.auto_extract);
    }

    #[test]
    fn test_resolved_global_overrides_defaults() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        crate::db::config::set_global_config(&conn, "general.autonomy", "cautious").unwrap();
        crate::db::config::set_global_config(&conn, "noise.threshold", "3").unwrap();
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert_eq!(cfg.general.autonomy, "cautious");
        assert_eq!(cfg.noise.threshold, 3);
    }

    #[test]
    fn test_resolved_session_overrides_global() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        crate::db::config::set_global_config(&conn, "general.autonomy", "cautious").unwrap();
        crate::db::config::set_session_config(&conn, "s1", "general.autonomy", "autonomous")
            .unwrap();
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert_eq!(cfg.general.autonomy, "autonomous");
    }

    #[test]
    fn test_resolved_bool_parsing() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        crate::db::config::set_global_config(&conn, "general.auto_extract", "false").unwrap();
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert!(!cfg.general.auto_extract);
    }

    #[test]
    fn test_resolved_json_array_parsing() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        crate::db::config::set_session_config(
            &conn,
            "s1",
            "tools.aliases",
            r#"["nmap","curl"]"#,
        )
        .unwrap();
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert_eq!(cfg.tools.aliases, vec!["nmap", "curl"]);
    }

    #[test]
    fn test_resolved_unknown_key_ignored() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        crate::db::config::set_global_config(&conn, "unknown.key", "whatever").unwrap();
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert_eq!(cfg.general.autonomy, "balanced");
    }

    #[test]
    fn test_get_key() {
        let cfg = Config::default();
        assert_eq!(cfg.get_key("general.autonomy"), Some("balanced".to_string()));
        assert_eq!(cfg.get_key("noise.threshold"), Some("5".to_string()));
        assert!(cfg.get_key("nonexistent").is_none());
    }

    #[test]
    fn test_apply_key_string() {
        let mut cfg = Config::default();
        cfg.apply_key("general.llm_model", "llama3");
        assert_eq!(cfg.general.llm_model, "llama3");
    }

    #[test]
    fn test_apply_key_bool() {
        let mut cfg = Config::default();
        cfg.apply_key("scope.strict", "true");
        assert!(cfg.scope.strict);
    }

    #[test]
    fn test_apply_key_json_array() {
        let mut cfg = Config::default();
        cfg.apply_key("scope.allowed_hosts", r#"["10.0.0.1","10.0.0.2"]"#);
        assert_eq!(
            cfg.scope.allowed_hosts,
            vec!["10.0.0.1".to_string(), "10.0.0.2".to_string()]
        );
    }

    #[test]
    fn test_resolved_global_only() {
        let conn = test_conn();
        crate::db::config::set_global_config(&conn, "general.autonomy", "cautious").unwrap();
        let cfg = Config::resolved_global(&conn).unwrap();
        assert_eq!(cfg.general.autonomy, "cautious");
        assert_eq!(cfg.noise.threshold, 5);
    }

    #[test]
    fn test_set_get_roundtrip() {
        let conn = test_conn();
        insert_session(&conn, "s1");
        crate::db::config::set_session_config(&conn, "s1", "general.llm_model", "gpt-4").unwrap();
        let cfg = Config::resolved(&conn, "s1").unwrap();
        assert_eq!(cfg.get_key("general.llm_model"), Some("gpt-4".to_string()));
    }
}
