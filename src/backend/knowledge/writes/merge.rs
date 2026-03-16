use super::super::KnowledgeBase;

impl KnowledgeBase {
    pub fn merge_from(&mut self, other: &KnowledgeBase) {
        for host in &other.discovered_hosts {
            if let Some(existing) = self.discovered_hosts.iter_mut().find(|h| h.ip == host.ip) {
                for port in &host.ports {
                    if !existing.ports.contains(port) {
                        existing.ports.push(*port);
                    }
                }
                for service in &host.services {
                    if !existing.services.contains(service) {
                        existing.services.push(service.clone());
                    }
                }
            } else {
                self.discovered_hosts.push(host.clone());
            }
        }

        for cred in &other.credentials {
            let exists = self
                .credentials
                .iter()
                .any(|c| c.username == cred.username && c.password == cred.password);
            if !exists {
                self.credentials.push(cred.clone());
            }
        }

        for level in &other.access_levels {
            if !self.access_levels.contains(level) {
                self.access_levels.push(level.clone());
            }
        }

        for path in &other.attack_paths {
            if !self.attack_paths.contains(path) {
                self.attack_paths.push(path.clone());
            }
        }

        for flag in &other.flags {
            if !self.flags.contains(flag) {
                self.flags.push(flag.clone());
            }
        }

        for attempt in &other.failed_attempts {
            self.failed_attempts.push(attempt.clone());
        }

        for note in &other.notes {
            if !self.notes.contains(note) {
                self.notes.push(note.clone());
            }
        }

        for task in &other.completed_tasks {
            self.completed_tasks.push(task.clone());
        }

        for task in &other.failed_tasks {
            self.failed_tasks.push(task.clone());
        }

        for def_name in &other.custom_definitions_used {
            if !self.custom_definitions_used.contains(def_name) {
                self.custom_definitions_used.push(def_name.clone());
            }
        }
    }
}
