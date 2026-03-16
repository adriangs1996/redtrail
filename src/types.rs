use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub base_url: Option<String>,
    pub hosts: Vec<String>,
    pub exec_mode: ExecMode,
    pub auth_token: Option<String>,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecMode {
    Docker,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub username: String,
    pub password: Option<String>,
    pub hash: Option<String>,
    pub service: String,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VulnType {
    SqlInjection,
    StoredXSS,
    ReflectedXSS,
    IDOR,
    PrivilegeEscalation,
    CommandInjection,
    InformationDisclosure,
    WeakCredentials,
    DefaultCredentials,
    AnonymousAccess,
    MissingAuthentication,
    SSRF,
    CSRF,
    SSTI,
    FileUpload,
    Deserialization,
    PathTraversal,
    BufferOverflow,
    FormatString,
    KerberosAttack,
    ContainerEscape,
    DockerSocketExposure,
    KubernetesExposure,
    SupplyChainCompromise,
    CiCdInjection,
    PromptInjection,
    ToolPoisoning,
    DNSZoneTransfer,
    SMBMisconfiguration,
    PassTheHash,
    GoldenTicket,
    DependencyConfusion,
    DataExfiltration,
    InsecureConfiguration,
    SensitiveDataExposure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub vuln_type: VulnType,
    pub severity: Severity,
    pub endpoint: String,
    pub evidence: Vec<Evidence>,
    pub description: String,
    pub fix_suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub request: HttpRequest,
    pub response: HttpResponse,
    pub payload: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackSurface {
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub url: String,
    pub method: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub location: ParameterLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ParameterLocation {
    Query,
    Body,
    Header,
    Path,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_construction() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: Some("token123".to_string()),
            scope: vec!["/api".to_string(), "/admin".to_string()],
        };
        assert_eq!(target.base_url, Some("https://example.com".to_string()));
        assert_eq!(target.auth_token, Some("token123".to_string()));
        assert_eq!(target.scope.len(), 2);
    }

    #[test]
    fn test_target_without_auth() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec![],
        };
        assert!(target.auth_token.is_none());
    }

    #[test]
    fn test_target_serialization() {
        let target = Target {
            base_url: Some("https://example.com".to_string()),
            hosts: vec![],
            exec_mode: ExecMode::Local,
            auth_token: None,
            scope: vec!["/*".to_string()],
        };
        let json = serde_json::to_string(&target).unwrap();
        let deserialized: Target = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.base_url, target.base_url);
        assert_eq!(deserialized.scope, target.scope);
    }

    #[test]
    fn test_http_request_serialization() {
        let req = HttpRequest {
            method: "POST".to_string(),
            url: "https://example.com/api/login".to_string(),
            headers: HashMap::from([("Content-Type".to_string(), "application/json".to_string())]),
            body: Some(r#"{"user":"admin"}"#.to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: HttpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.method, "POST");
        assert_eq!(deserialized.url, req.url);
    }

    #[test]
    fn test_http_response_serialization() {
        let resp = HttpResponse {
            status_code: 200,
            headers: HashMap::new(),
            body: "OK".to_string(),
            elapsed_ms: 42,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: HttpResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status_code, 200);
        assert_eq!(deserialized.elapsed_ms, 42);
    }

    #[test]
    fn test_vuln_type_serialization() {
        let vuln = VulnType::SqlInjection;
        let json = serde_json::to_string(&vuln).unwrap();
        assert_eq!(json, r#""SqlInjection""#);
        let deserialized: VulnType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, VulnType::SqlInjection);
    }

    #[test]
    fn test_severity_ordering() {
        let severities = vec![
            Severity::Info,
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ];
        for s in &severities {
            let json = serde_json::to_string(s).unwrap();
            let deserialized: Severity = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, s);
        }
    }

    #[test]
    fn test_finding_construction() {
        let finding = Finding {
            vuln_type: VulnType::ReflectedXSS,
            severity: Severity::High,
            endpoint: "/search".to_string(),
            evidence: vec![],
            description: "Reflected XSS in search parameter".to_string(),
            fix_suggestion: "Sanitize user input".to_string(),
        };
        assert_eq!(finding.vuln_type, VulnType::ReflectedXSS);
        assert_eq!(finding.severity, Severity::High);
        assert!(finding.evidence.is_empty());
    }

    #[test]
    fn test_finding_with_evidence_serialization() {
        let finding = Finding {
            vuln_type: VulnType::SqlInjection,
            severity: Severity::Critical,
            endpoint: "/api/users".to_string(),
            evidence: vec![Evidence {
                request: HttpRequest {
                    method: "GET".to_string(),
                    url: "/api/users?id=1' OR '1'='1".to_string(),
                    headers: HashMap::new(),
                    body: None,
                },
                response: HttpResponse {
                    status_code: 200,
                    headers: HashMap::new(),
                    body: "all users returned".to_string(),
                    elapsed_ms: 150,
                },
                payload: "1' OR '1'='1".to_string(),
                description: "SQL injection in id parameter".to_string(),
            }],
            description: "SQL injection vulnerability".to_string(),
            fix_suggestion: "Use parameterized queries".to_string(),
        };
        let json = serde_json::to_string(&finding).unwrap();
        let deserialized: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.evidence.len(), 1);
        assert_eq!(deserialized.evidence[0].payload, "1' OR '1'='1");
    }

    #[test]
    fn test_attack_surface_serialization() {
        let surface = AttackSurface {
            endpoints: vec![Endpoint {
                url: "/api/users".to_string(),
                method: "GET".to_string(),
                parameters: vec![
                    Parameter {
                        name: "id".to_string(),
                        location: ParameterLocation::Query,
                    },
                    Parameter {
                        name: "Authorization".to_string(),
                        location: ParameterLocation::Header,
                    },
                ],
            }],
        };
        let json = serde_json::to_string(&surface).unwrap();
        let deserialized: AttackSurface = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.endpoints.len(), 1);
        assert_eq!(deserialized.endpoints[0].parameters.len(), 2);
        assert_eq!(
            deserialized.endpoints[0].parameters[0].location,
            ParameterLocation::Query
        );
    }

    #[test]
    fn test_parameter_location_variants() {
        let locations = vec![
            ParameterLocation::Query,
            ParameterLocation::Body,
            ParameterLocation::Header,
            ParameterLocation::Path,
        ];
        for loc in &locations {
            let json = serde_json::to_string(loc).unwrap();
            let deserialized: ParameterLocation = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, loc);
        }
    }
}
