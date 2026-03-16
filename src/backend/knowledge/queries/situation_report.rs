use std::fmt::Write as _;

use super::super::KnowledgeBase;
use super::super::types::command::CommandSource;
use super::super::types::task::TaskStatus;

impl KnowledgeBase {
    pub fn situation_report(&self) -> String {
        let mut report = String::from("# Situation Report\n");

        if !self.discovered_hosts.is_empty() {
            report.push_str("\n## Discovered Hosts\n");
            for h in &self.discovered_hosts {
                let _ = write!(report, "- {}", h.ip);
                if !h.ports.is_empty() {
                    let ports: Vec<String> = h.ports.iter().map(|p| p.to_string()).collect();
                    let _ = write!(report, " ports=[{}]", ports.join(","));
                }
                if !h.services.is_empty() {
                    let _ = write!(report, " services=[{}]", h.services.join(","));
                }
                if let Some(os) = &h.os {
                    let _ = write!(report, " os={os}");
                }
                report.push('\n');
            }
        }

        if !self.credentials.is_empty() {
            report.push_str("\n## Credentials\n");
            for c in &self.credentials {
                let _ = writeln!(
                    report,
                    "- {}:{} (service={}, host={})",
                    c.username,
                    c.password.as_deref().unwrap_or("***"),
                    if c.service.is_empty() {
                        "any"
                    } else {
                        &c.service
                    },
                    if c.host.is_empty() { "any" } else { &c.host },
                );
            }
        }

        if !self.access_levels.is_empty() {
            report.push_str("\n## Access Levels\n");
            for a in &self.access_levels {
                let _ = writeln!(
                    report,
                    "- {}@{} [{}] via {}",
                    a.user, a.host, a.privilege_level, a.method
                );
            }
        }

        if !self.flags.is_empty() {
            report.push_str("\n## Captured Flags\n");
            for f in &self.flags {
                let _ = writeln!(report, "- {f}");
            }
        }

        if !self.attack_paths.is_empty() {
            report.push_str("\n## Successful Attack Paths\n");
            for p in &self.attack_paths {
                let _ = writeln!(report, "- {}", p.description);
            }
        }

        if !self.completed_tasks.is_empty() {
            report.push_str("\n## Completed Tasks\n");
            for t in &self.completed_tasks {
                let _ = writeln!(
                    report,
                    "- [{}] {} on {} ({}s): {}",
                    t.task_type, t.task_name, t.target_host, t.duration_secs, t.key_findings
                );
            }
        }

        if !self.failed_tasks.is_empty() {
            report.push_str("\n## Failed Tasks\n");
            for t in &self.failed_tasks {
                let status_label = match t.status {
                    TaskStatus::Failed => "FAILED",
                    TaskStatus::Timeout => "TIMEOUT",
                    TaskStatus::Completed => "COMPLETED",
                };
                let _ = writeln!(
                    report,
                    "- [{}] {} on {} ({}s) {}: {}",
                    t.task_type,
                    t.task_name,
                    t.target_host,
                    t.duration_secs,
                    status_label,
                    t.key_findings
                );
            }
        }

        if !self.failed_attempts.is_empty() {
            report.push_str("\n## Failed Attempts\n");
            for f in &self.failed_attempts {
                let _ = writeln!(report, "- {} on {}: {}", f.tool, f.target, f.description);
            }
        }

        if !self.notes.is_empty() {
            report.push_str("\n## Notes\n");
            for n in &self.notes {
                let _ = writeln!(report, "- {n}");
            }
        }

        if !self.activated_specialists.is_empty() {
            report.push_str("\n## Activated Specialists\n");
            let _ = writeln!(report, "- {}", self.activated_specialists.join(", "));
        }

        if !self.command_history.is_empty() {
            report.push_str("\n## Command History\n");
            for cmd in &self.command_history {
                let source = match cmd.source {
                    CommandSource::Terminal => "term",
                    CommandSource::Tool => "tool",
                };
                let status = match cmd.exit_code {
                    Some(0) => "ok".to_string(),
                    Some(c) => format!("exit:{c}"),
                    None => "?".to_string(),
                };
                let _ = writeln!(
                    report,
                    "- [{}][{}] `{}` ({})",
                    source, status, cmd.command, cmd.timestamp
                );
            }
        }

        report
    }
}
