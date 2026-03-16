use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WafType {
    ModSecurity,
    Cloudflare,
    AwsWaf,
    Imperva,
    F5BigIp,
    Akamai,
    Unknown(String),
}

impl Default for WafType {
    fn default() -> Self {
        Self::Unknown("unknown".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DetectedWaf {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub waf_type: WafType,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub blocked_payloads: Vec<String>,
    #[serde(default)]
    pub successful_bypasses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimit {
    pub host: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub max_requests: u32,
    #[serde(default)]
    pub window_secs: u32,
    #[serde(default)]
    pub limit_status: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IdsSensitivity {
    #[default]
    None,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DetectionCost {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub cost: f32,
    #[serde(default)]
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BypassTechnique {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub effective_against: Vec<WafType>,
    #[serde(default)]
    pub success_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefenderModel {
    #[serde(default)]
    pub detected_wafs: Vec<DetectedWaf>,
    #[serde(default)]
    pub ids_sensitivity: IdsSensitivity,
    #[serde(default)]
    pub rate_limits: Vec<RateLimit>,
    #[serde(default)]
    pub blocked_payloads: Vec<String>,
    #[serde(default = "default_noise_budget")]
    pub noise_budget: f32,
    #[serde(default)]
    pub action_costs: Vec<DetectionCost>,
    #[serde(default)]
    pub bypass_techniques: Vec<BypassTechnique>,
}

fn default_noise_budget() -> f32 {
    1.0
}

impl Default for DefenderModel {
    fn default() -> Self {
        Self {
            detected_wafs: Vec::new(),
            ids_sensitivity: IdsSensitivity::None,
            rate_limits: Vec::new(),
            blocked_payloads: Vec::new(),
            noise_budget: 1.0,
            action_costs: Vec::new(),
            bypass_techniques: Vec::new(),
        }
    }
}

impl DefenderModel {
    pub fn record_block(&mut self, host: &str, payload: &str, response_status: u16) {
        self.blocked_payloads.push(payload.to_string());

        if let Some(waf) = self.detected_wafs.iter_mut().find(|w| w.host == host) {
            waf.blocked_payloads.push(payload.to_string());
            waf.confidence = (waf.confidence + 0.1).min(1.0);
        } else {
            let waf_type = Self::classify_waf_from_block(response_status);
            self.detected_wafs.push(DetectedWaf {
                host: host.to_string(),
                port: None,
                waf_type,
                confidence: 0.5,
                blocked_payloads: vec![payload.to_string()],
                successful_bypasses: vec![],
            });
        }

        self.noise_budget = (self.noise_budget - 0.1).max(0.0);
    }

    pub fn record_bypass(&mut self, host: &str, technique: &str) {
        if let Some(waf) = self.detected_wafs.iter_mut().find(|w| w.host == host)
            && !waf.successful_bypasses.contains(&technique.to_string())
        {
            waf.successful_bypasses.push(technique.to_string());
        }
    }

    fn classify_waf_from_block(status: u16) -> WafType {
        match status {
            403 => WafType::Unknown("generic_403".into()),
            406 => WafType::ModSecurity,
            429 => WafType::Unknown("rate_limiter".into()),
            503 => WafType::Unknown("overload_protection".into()),
            _ => WafType::Unknown(format!("status_{status}")),
        }
    }

    pub fn detection_cost_for(&self, action: &str) -> f32 {
        if let Some(cost) = self.action_costs.iter().find(|c| c.action == action) {
            return cost.cost;
        }
        Self::default_detection_cost(action)
    }

    fn default_detection_cost(action: &str) -> f32 {
        match action {
            "differential_probe" | "stack_fingerprint" => 0.1,
            "port_scan" | "service_probe" => 0.2,
            "dir_bust" | "web_enum" => 0.3,
            "sql_injection" | "command_injection" | "web_exploit" => 0.5,
            "brute_force" => 0.8,
            "exploit_hypothesis" => 0.4,
            "try_credentials" => 0.2,
            "map_architecture" => 0.15,
            "privesc_enum" | "suid_exploit" | "read_flag" => 0.05,
            _ => 0.3,
        }
    }

    pub fn is_action_allowed(&self, action: &str) -> bool {
        self.detection_cost_for(action) <= self.noise_budget
    }

    pub fn suggest_bypasses(&self, host: &str) -> Vec<&BypassTechnique> {
        let waf = match self.detected_wafs.iter().find(|w| w.host == host) {
            Some(w) => w,
            None => return vec![],
        };

        self.bypass_techniques
            .iter()
            .filter(|t| {
                t.effective_against.contains(&waf.waf_type) || t.effective_against.is_empty()
            })
            .collect()
    }

    pub fn wafs_for_host(&self, host: &str) -> Vec<&DetectedWaf> {
        self.detected_wafs
            .iter()
            .filter(|w| w.host == host)
            .collect()
    }

    pub fn record_rate_limit(
        &mut self,
        host: &str,
        endpoint: Option<&str>,
        max_requests: u32,
        window_secs: u32,
        limit_status: u16,
    ) {
        self.rate_limits.push(RateLimit {
            host: host.to_string(),
            endpoint: endpoint.map(|e| e.to_string()),
            max_requests,
            window_secs,
            limit_status,
        });
    }

    pub fn update_ids_sensitivity(&mut self, sensitivity: IdsSensitivity) {
        self.ids_sensitivity = sensitivity;
    }

    pub fn with_default_bypasses() -> Self {
        Self {
            bypass_techniques: vec![
                BypassTechnique {
                    name: "case_alternation".into(),
                    description: "Alternate upper/lower case in SQL keywords (e.g., SeLeCt)".into(),
                    effective_against: vec![
                        WafType::ModSecurity,
                        WafType::Unknown("generic_403".into()),
                    ],
                    success_rate: 0.6,
                },
                BypassTechnique {
                    name: "url_double_encoding".into(),
                    description: "Double URL-encode special characters".into(),
                    effective_against: vec![WafType::ModSecurity, WafType::Cloudflare],
                    success_rate: 0.4,
                },
                BypassTechnique {
                    name: "comment_injection".into(),
                    description: "Insert SQL comments between keywords (e.g., SEL/**/ECT)".into(),
                    effective_against: vec![WafType::ModSecurity],
                    success_rate: 0.5,
                },
                BypassTechnique {
                    name: "http_parameter_pollution".into(),
                    description: "Duplicate parameters to confuse WAF parsing".into(),
                    effective_against: vec![],
                    success_rate: 0.3,
                },
                BypassTechnique {
                    name: "chunked_transfer".into(),
                    description: "Use chunked Transfer-Encoding to split payloads".into(),
                    effective_against: vec![WafType::AwsWaf, WafType::Cloudflare],
                    success_rate: 0.35,
                },
                BypassTechnique {
                    name: "newline_injection".into(),
                    description: "Use \\r\\n or \\n within payloads to break signature matching"
                        .into(),
                    effective_against: vec![
                        WafType::ModSecurity,
                        WafType::Unknown("generic_403".into()),
                    ],
                    success_rate: 0.45,
                },
            ],
            ..Self::default()
        }
    }

    pub fn adjusted_priority(&self, base_priority: f32, action: &str) -> f32 {
        let detection = self.detection_cost_for(action);
        let penalty = detection * (1.0 - self.noise_budget);
        base_priority * (1.0 - penalty)
    }
}
