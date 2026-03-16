use super::super::KnowledgeBase;

impl KnowledgeBase {
    pub fn to_context_summary(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();

        if !self.discovered_hosts.is_empty() {
            let hosts: Vec<String> = self
                .discovered_hosts
                .iter()
                .map(|h| {
                    if h.ports.is_empty() {
                        h.ip.clone()
                    } else {
                        let ports: Vec<String> = h.ports.iter().map(|p| p.to_string()).collect();
                        format!("{}:[{}]", h.ip, ports.join(","))
                    }
                })
                .collect();
            parts.push(format!("Discovered hosts: {}", hosts.join(", ")));
        }

        if !self.credentials.is_empty() {
            let creds: Vec<String> = self
                .credentials
                .iter()
                .map(|c| format!("{}:{}", c.username, c.password.as_deref().unwrap_or("***")))
                .collect();
            parts.push(format!("Credentials: {}", creds.join(", ")));
        }

        if !self.access_levels.is_empty() {
            let levels: Vec<String> = self
                .access_levels
                .iter()
                .map(|a| format!("{}@{} ({})", a.user, a.host, a.privilege_level))
                .collect();
            parts.push(format!("Access levels: {}", levels.join(", ")));
        }

        if !self.flags.is_empty() {
            parts.push(format!("Captured flags: {}", self.flags.join(", ")));
        }

        if !self.attack_paths.is_empty() {
            let paths: Vec<&str> = self
                .attack_paths
                .iter()
                .map(|p| p.description.as_str())
                .collect();
            parts.push(format!("Successful attack paths: {}", paths.join("; ")));
        }

        if !self.failed_attempts.is_empty() {
            let fails: Vec<String> = self
                .failed_attempts
                .iter()
                .map(|f| format!("{} on {} ({})", f.tool, f.target, f.description))
                .collect();
            parts.push(format!("Failed attempts: {}", fails.join("; ")));
        }

        if !self.notes.is_empty() {
            parts.push(format!("Notes: {}", self.notes.join("; ")));
        }

        if !self.activated_specialists.is_empty() {
            parts.push(format!(
                "Activated specialists: {}",
                self.activated_specialists.join(", ")
            ));
        }

        if !self.command_history.is_empty() {
            let cmds: Vec<String> = self
                .command_history
                .iter()
                .rev()
                .take(10)
                .map(|c| c.command.clone())
                .collect();
            parts.push(format!(
                "Recent commands (last {}): {}",
                cmds.len(),
                cmds.join("; ")
            ));
        }

        format!("\n\n## Current Knowledge Base\n{}", parts.join("\n"))
    }
}
