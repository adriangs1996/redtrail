use regex::Regex;
use std::sync::LazyLock;

use super::super::KnowledgeBase;
use super::super::types::host::HostInfo;
use super::super::types::session::GoalType;

static FLAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"FLAG\{[^}]+\}").expect("invalid FLAG regex"));

static IP_PORT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}):(\d{1,5})").expect("invalid IP:port regex")
});

static IP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})\b").expect("invalid IP regex")
});

static CRED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:username|user|login)\s*[:=]\s*(\S+)\s+(?:password|pass|pwd)\s*[:=]\s*(\S+)")
        .expect("invalid credential regex")
});

impl KnowledgeBase {
    pub fn extract_from_output(&mut self, tool_name: &str, target: &str, output: &str) {
        self.extract_flags(output);
        self.extract_credentials(output);
        self.extract_hosts(output);
        self.extract_failed_attempts(tool_name, target, output);
    }

    pub fn sync_flag_regex(&mut self) {
        if let GoalType::CaptureFlags {
            ref flag_pattern, ..
        } = self.goal.goal_type
            && let Ok(re) = Regex::new(flag_pattern)
        {
            self.flag_regex = Some(re);
        }
    }

    fn extract_flags(&mut self, output: &str) {
        let re = self.flag_regex.as_ref().unwrap_or(&FLAG_RE);
        for cap in re.find_iter(output) {
            let flag = cap.as_str().to_string();
            if !self.flags.contains(&flag) {
                self.flags.push(flag);
            }
        }
    }

    fn extract_credentials(&mut self, output: &str) {
        for cap in CRED_RE.captures_iter(output) {
            let username = cap[1].to_string();
            let password = cap[2].to_string();
            let already_exists = self.credentials.iter().any(|c| {
                c.username == username && c.password.as_deref() == Some(password.as_str())
            });
            if !already_exists {
                self.credentials.push(crate::types::Credential {
                    username,
                    password: Some(password),
                    hash: None,
                    service: String::new(),
                    host: String::new(),
                });
            }
        }
    }

    fn extract_hosts(&mut self, output: &str) {
        self.extract_hosts_from_json(output);

        for cap in IP_PORT_RE.captures_iter(output) {
            let ip = cap[1].to_string();
            let port: u16 = match cap[2].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            self.add_host_port(&ip, port);
        }

        for cap in IP_RE.captures_iter(output) {
            let ip = cap[1].to_string();
            if !self.discovered_hosts.iter().any(|h| h.ip == ip) {
                self.discovered_hosts.push(HostInfo {
                    ip,
                    ports: vec![],
                    services: vec![],
                    os: None,
                });
            }
        }
    }

    fn extract_hosts_from_json(&mut self, output: &str) {
        let parsed: serde_json::Value = match serde_json::from_str(output) {
            Ok(v) => v,
            Err(_) => return,
        };

        if let Some(hosts) = parsed.get("hosts").and_then(|h| h.as_array()) {
            for host in hosts {
                let ip = match host.get("ip").and_then(|v| v.as_str()) {
                    Some(ip) => ip.to_string(),
                    None => continue,
                };

                if let Some(ports) = host.get("ports").and_then(|p| p.as_array()) {
                    for port_obj in ports {
                        let port =
                            port_obj.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                        let service = port_obj
                            .get("service")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let state = port_obj
                            .get("state")
                            .and_then(|s| s.as_str())
                            .unwrap_or("unknown");

                        if port > 0 && state == "open" {
                            self.add_host_port(&ip, port);
                            if !service.is_empty() {
                                self.add_host_service(&ip, &service);
                            }
                        }
                    }
                }

                if !self.discovered_hosts.iter().any(|h| h.ip == ip) {
                    self.discovered_hosts.push(HostInfo {
                        ip,
                        ports: vec![],
                        services: vec![],
                        os: None,
                    });
                }
            }
        }
    }

    pub(crate) fn add_host_port(&mut self, ip: &str, port: u16) {
        if let Some(host) = self.discovered_hosts.iter_mut().find(|h| h.ip == ip) {
            if !host.ports.contains(&port) {
                host.ports.push(port);
            }
        } else {
            self.discovered_hosts.push(HostInfo {
                ip: ip.to_string(),
                ports: vec![port],
                services: vec![],
                os: None,
            });
        }
    }

    pub(crate) fn add_host_service(&mut self, ip: &str, service: &str) {
        if let Some(host) = self.discovered_hosts.iter_mut().find(|h| h.ip == ip)
            && !host.services.iter().any(|s| s == service)
        {
            host.services.push(service.to_string());
        }
    }

    fn extract_failed_attempts(&mut self, tool_name: &str, target: &str, output: &str) {
        let lower = output.to_lowercase();
        if lower.contains("permission denied")
            || lower.contains("access denied")
            || lower.contains("authentication failed")
            || lower.contains("unauthorized")
        {
            self.failed_attempts
                .push(super::super::types::host::FailedAttempt {
                    tool: tool_name.to_string(),
                    target: target.to_string(),
                    description: Self::extract_denial_context(&lower),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                });
        }
    }

    fn extract_denial_context(lower: &str) -> String {
        for pattern in [
            "permission denied",
            "access denied",
            "authentication failed",
            "unauthorized",
        ] {
            if let Some(pos) = lower.find(pattern) {
                let start = pos.saturating_sub(20);
                let end = (pos + pattern.len() + 20).min(lower.len());
                return lower[start..end].to_string();
            }
        }
        "access denied".to_string()
    }
}
