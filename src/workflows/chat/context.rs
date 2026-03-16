use crate::backend::knowledge::KnowledgeBase;
use crate::types::Target;

pub fn gather_context(kb: &KnowledgeBase, target: &Target, session_name: &str) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Session: {}", session_name));
    parts.push(format!("Target: {}", target.hosts.join(", ")));

    if !kb.discovered_hosts.is_empty() {
        parts.push(format!("Discovered hosts ({}):", kb.discovered_hosts.len()));
        for h in kb.discovered_hosts.iter().take(20) {
            parts.push(format!("  - {}", h.ip));
        }
    }

    if !kb.credentials.is_empty() {
        parts.push(format!("Credentials ({}):", kb.credentials.len()));
        for c in kb.credentials.iter().take(10) {
            parts.push(format!("  - {}:{}", c.username, c.password.as_deref().unwrap_or("*")));
        }
    }

    if !kb.flags.is_empty() {
        parts.push(format!("Flags ({}):", kb.flags.len()));
        for f in &kb.flags {
            parts.push(format!("  - {}", f));
        }
    }

    if !kb.access_levels.is_empty() {
        parts.push(format!("Access levels ({}):", kb.access_levels.len()));
        for a in kb.access_levels.iter().take(10) {
            parts.push(format!("  - {}@{}: {}", a.user, a.host, a.privilege_level));
        }
    }

    parts.join("\n")
}
