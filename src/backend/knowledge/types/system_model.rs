use serde::{Deserialize, Serialize};

use super::deductive::DeductiveLayer;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComponentType {
    #[serde(alias = "web_app", alias = "web_server", alias = "webapp")]
    WebApp,
    #[serde(alias = "database", alias = "db")]
    Database,
    #[serde(alias = "auth_service", alias = "auth")]
    AuthService,
    #[serde(alias = "file_server", alias = "ftp_server", alias = "ssh_server")]
    FileServer,
    #[serde(alias = "mail_server")]
    MailServer,
    #[serde(alias = "dns_server")]
    DnsServer,
    #[serde(alias = "cache_store")]
    CacheStore,
    #[serde(alias = "container_runtime")]
    ContainerRuntime,
    Custom(String),
}

impl Default for ComponentType {
    fn default() -> Self {
        Self::Custom("unknown".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StackFingerprint {
    pub server: Option<String>,
    pub framework: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub technologies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoint {
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub parameters: Vec<String>,
    #[serde(default)]
    pub auth_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemComponent {
    pub id: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub component_type: ComponentType,
    #[serde(default, deserialize_with = "deserialize_stack_lenient")]
    pub stack: StackFingerprint,
    #[serde(default, deserialize_with = "deserialize_entry_points_lenient")]
    pub entry_points: Vec<EntryPoint>,
    #[serde(default)]
    pub confidence: f32,
}

fn deserialize_stack_lenient<'de, D>(deserializer: D) -> Result<StackFingerprint, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match serde_json::from_value::<StackFingerprint>(value) {
        Ok(stack) => Ok(stack),
        Err(_) => Ok(StackFingerprint::default()),
    }
}

fn deserialize_entry_points_lenient<'de, D>(deserializer: D) -> Result<Vec<EntryPoint>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let arr = match value.as_array() {
        Some(arr) => arr,
        None => return Ok(Vec::new()),
    };
    let mut result = Vec::new();
    for item in arr {
        if let Some(s) = item.as_str() {
            result.push(EntryPoint {
                path: s.to_string(),
                method: "GET".into(),
                parameters: Vec::new(),
                auth_required: false,
            });
        } else if let Ok(ep) = serde_json::from_value::<EntryPoint>(item.clone()) {
            result.push(ep);
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustBoundary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub components: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataFlow {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub data_type: String,
    #[serde(default)]
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum HypothesisCategory {
    #[default]
    Boundary,
    Input,
    State,
    Confidentiality,
    Logic,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum HypothesisStatus {
    #[default]
    Proposed,
    Probing,
    Confirmed,
    Refuted,
    Exploited,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProbeResult {
    #[serde(default)]
    pub probe_type: String,
    #[serde(default)]
    pub request_summary: String,
    #[serde(default)]
    pub response_status: u16,
    #[serde(default)]
    pub response_length: usize,
    #[serde(default)]
    pub timing_ms: u64,
    #[serde(default)]
    pub anomaly_detected: bool,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Hypothesis {
    pub id: String,
    #[serde(default)]
    pub component_id: String,
    #[serde(default)]
    pub category: HypothesisCategory,
    pub statement: String,
    #[serde(default)]
    pub status: HypothesisStatus,
    #[serde(default)]
    pub probes: Vec<ProbeResult>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub task_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemModel {
    pub components: Vec<SystemComponent>,
    pub trust_boundaries: Vec<TrustBoundary>,
    pub data_flows: Vec<DataFlow>,
    pub hypotheses: Vec<Hypothesis>,
    pub model_confidence: f32,
    pub current_layer: DeductiveLayer,
}

impl Default for SystemModel {
    fn default() -> Self {
        Self {
            components: Vec::new(),
            trust_boundaries: Vec::new(),
            data_flows: Vec::new(),
            hypotheses: Vec::new(),
            model_confidence: 0.0,
            current_layer: DeductiveLayer::Modeling,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentUpdate {
    #[serde(default)]
    pub stack: Option<StackFingerprint>,
    #[serde(default)]
    pub component_type: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub add_entry_points: Vec<EntryPoint>,
}

impl SystemModel {
    pub fn add_component(&mut self, component: SystemComponent) {
        self.components.push(component);
    }

    pub fn update_component(&mut self, id: &str, update: ComponentUpdate) {
        if let Some(comp) = self.components.iter_mut().find(|c| c.id == id) {
            if let Some(stack) = update.stack {
                comp.stack = stack;
            }
            if let Some(ct) = update.component_type {
                comp.component_type = match ct.as_str() {
                    "WebApp" => ComponentType::WebApp,
                    "Database" => ComponentType::Database,
                    "AuthService" => ComponentType::AuthService,
                    "FileServer" => ComponentType::FileServer,
                    "MailServer" => ComponentType::MailServer,
                    "DnsServer" => ComponentType::DnsServer,
                    "CacheStore" => ComponentType::CacheStore,
                    "ContainerRuntime" => ComponentType::ContainerRuntime,
                    other => ComponentType::Custom(other.to_string()),
                };
            }
            if let Some(conf) = update.confidence {
                comp.confidence = conf;
            }
            for ep in update.add_entry_points {
                comp.entry_points.push(ep);
            }
        }
    }

    pub fn add_hypothesis(&mut self, hypothesis: Hypothesis) {
        self.hypotheses.push(hypothesis);
    }

    pub fn update_hypothesis_status(&mut self, id: &str, status: HypothesisStatus) {
        if let Some(h) = self.hypotheses.iter_mut().find(|h| h.id == id) {
            h.status = status;
        }
    }

    pub fn advance_layer(&mut self, layer: DeductiveLayer) {
        self.current_layer = layer;
    }

    pub fn get_hypothesis(&self, id: &str) -> Option<&Hypothesis> {
        self.hypotheses.iter().find(|h| h.id == id)
    }

    pub fn get_hypothesis_mut(&mut self, id: &str) -> Option<&mut Hypothesis> {
        self.hypotheses.iter_mut().find(|h| h.id == id)
    }
}
