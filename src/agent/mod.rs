pub mod attack_graph;
pub mod claude_executor;
pub mod driver;
pub mod flags;
pub mod knowledge;
pub mod llm;
pub mod query_agent;
pub mod reactor;
pub mod strategist;
pub mod tools;

use serde::{Deserialize, Serialize};

use crate::types::{AttackSurface, Finding, Target};
use knowledge::KnowledgeBase;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub target: Target,
    pub findings: Vec<Finding>,
    pub attack_surface: Option<AttackSurface>,
    pub messages: Vec<AgentMessage>,
    pub turn_count: u32,
    pub knowledge: KnowledgeBase,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_message_construction() {
        let msg = AgentMessage {
            role: Role::User,
            content: "Scan the target".to_string(),
        };
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Scan the target");
    }

    #[test]
    fn test_agent_message_serialization() {
        let msg = AgentMessage {
            role: Role::Assistant,
            content: "Starting scan...".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::Assistant);
        assert_eq!(deserialized.content, msg.content);
    }

    #[test]
    fn test_role_variants_serialization() {
        let roles = vec![Role::System, Role::User, Role::Assistant, Role::Tool];
        for role in &roles {
            let json = serde_json::to_string(role).unwrap();
            let deserialized: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, role);
        }
    }

    #[test]
    fn test_tool_call_construction() {
        let call = ToolCall {
            tool_name: "http_request".to_string(),
            arguments: serde_json::json!({"url": "https://example.com", "method": "GET"}),
        };
        assert_eq!(call.tool_name, "http_request");
        assert_eq!(call.arguments["method"], "GET");
    }

    #[test]
    fn test_tool_call_serialization() {
        let call = ToolCall {
            tool_name: "sql_inject".to_string(),
            arguments: serde_json::json!({"payload": "' OR 1=1 --"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let deserialized: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tool_name, "sql_inject");
    }

    #[test]
    fn test_agent_state_construction() {
        let state = AgentState {
            target: Target {
                base_url: Some("https://example.com".to_string()),
                hosts: vec![],
                exec_mode: crate::types::ExecMode::Local,
                auth_token: None,
                scope: vec!["/*".to_string()],
            },
            findings: vec![],
            attack_surface: None,
            messages: vec![AgentMessage {
                role: Role::System,
                content: "You are a security scanner".to_string(),
            }],
            turn_count: 0,
            knowledge: KnowledgeBase::new(),
        };
        assert_eq!(
            state.target.base_url,
            Some("https://example.com".to_string())
        );
        assert!(state.findings.is_empty());
        assert!(state.attack_surface.is_none());
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.turn_count, 0);
    }

    #[test]
    fn test_agent_state_serialization() {
        let state = AgentState {
            target: Target {
                base_url: Some("https://target.com".to_string()),
                hosts: vec![],
                exec_mode: crate::types::ExecMode::Local,
                auth_token: Some("bearer-token".to_string()),
                scope: vec!["/api".to_string()],
            },
            findings: vec![],
            attack_surface: None,
            messages: vec![],
            turn_count: 5,
            knowledge: KnowledgeBase::new(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.target.base_url,
            Some("https://target.com".to_string())
        );
        assert_eq!(deserialized.turn_count, 5);
    }
}
