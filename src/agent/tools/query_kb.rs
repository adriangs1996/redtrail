use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::agent::knowledge::KnowledgeBase;
use crate::agent::tools::{RegisteredTool, ToolDef, ToolHandler};

#[derive(Deserialize, JsonSchema)]
pub struct QueryKbInput {
    /// Section to query: "full", "hosts", "credentials", "access", "flags",
    /// "attack_paths", "tasks", "notes", "commands"
    pub section: String,
}

pub fn query_kb(kb: Arc<RwLock<KnowledgeBase>>) -> RegisteredTool {
    use schemars::schema_for;

    let schema = schema_for!(QueryKbInput);
    let mut input_schema = serde_json::to_value(schema).unwrap();
    if let Some(obj) = input_schema.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
    }
    crate::agent::tools::simplify_schema(&mut input_schema);

    let def = ToolDef {
        name: "query_kb".to_string(),
        description: "Query the knowledge base for discovered intelligence (hosts, credentials, access levels, flags, attack paths, tasks, notes)".to_string(),
        input_schema,
    };

    let handler: ToolHandler = Box::new(move |raw: serde_json::Value| {
        let kb = kb.clone();
        Box::pin(async move {
            let input: QueryKbInput =
                serde_json::from_value(raw).map_err(|e| format!("invalid input: {e}"))?;

            let kb = kb.read().await;
            let section = input.section.to_lowercase();

            let result = match section.as_str() {
                "full" => kb.situation_report(),
                "hosts" => format_hosts(&kb),
                "credentials" | "creds" => format_credentials(&kb),
                "access" => format_access(&kb),
                "flags" => format_flags(&kb),
                "attack_paths" | "paths" => format_attack_paths(&kb),
                "tasks" => format_tasks(&kb),
                "notes" => format_notes(&kb),
                "commands" | "history" => format_commands(&kb),
                _ => format!(
                    "Unknown section '{}'. Available: full, hosts, credentials, access, flags, attack_paths, tasks, notes, commands",
                    input.section
                ),
            };

            Ok(serde_json::json!({ "data": result }))
        })
    });

    RegisteredTool { def, handler }
}

fn format_hosts(kb: &KnowledgeBase) -> String {
    if kb.discovered_hosts.is_empty() {
        return "No hosts discovered yet.".into();
    }
    let mut out = String::new();
    for h in &kb.discovered_hosts {
        out.push_str(&format!("- {}", h.ip));
        if !h.ports.is_empty() {
            let ports: Vec<String> = h.ports.iter().map(|p| p.to_string()).collect();
            out.push_str(&format!(" ports=[{}]", ports.join(",")));
        }
        if !h.services.is_empty() {
            out.push_str(&format!(" services=[{}]", h.services.join(",")));
        }
        if let Some(os) = &h.os {
            out.push_str(&format!(" os={os}"));
        }
        out.push('\n');
    }
    out
}

fn format_credentials(kb: &KnowledgeBase) -> String {
    if kb.credentials.is_empty() {
        return "No credentials discovered yet.".into();
    }
    kb.credentials
        .iter()
        .map(|c| {
            format!(
                "- {}:{} (service={}, host={})",
                c.username,
                c.password.as_deref().unwrap_or("***"),
                if c.service.is_empty() {
                    "any"
                } else {
                    &c.service
                },
                if c.host.is_empty() { "any" } else { &c.host },
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_access(kb: &KnowledgeBase) -> String {
    if kb.access_levels.is_empty() {
        return "No access levels recorded yet.".into();
    }
    kb.access_levels
        .iter()
        .map(|a| {
            format!(
                "- {}@{} [{}] via {}",
                a.user, a.host, a.privilege_level, a.method
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_flags(kb: &KnowledgeBase) -> String {
    if kb.flags.is_empty() {
        return "No flags captured yet.".into();
    }
    kb.flags
        .iter()
        .map(|f| format!("- {f}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_attack_paths(kb: &KnowledgeBase) -> String {
    if kb.attack_paths.is_empty() {
        return "No attack paths recorded yet.".into();
    }
    kb.attack_paths
        .iter()
        .map(|p| format!("- {}", p.description))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_tasks(kb: &KnowledgeBase) -> String {
    let mut out = String::new();
    if !kb.completed_tasks.is_empty() {
        out.push_str("Completed:\n");
        for t in &kb.completed_tasks {
            out.push_str(&format!(
                "- [{}] {} on {} ({}s): {}\n",
                t.task_type, t.task_name, t.target_host, t.duration_secs, t.key_findings
            ));
        }
    }
    if !kb.failed_tasks.is_empty() {
        out.push_str("Failed:\n");
        for t in &kb.failed_tasks {
            out.push_str(&format!(
                "- [{}] {} on {} ({}s): {}\n",
                t.task_type, t.task_name, t.target_host, t.duration_secs, t.key_findings
            ));
        }
    }
    if out.is_empty() {
        "No tasks recorded yet.".into()
    } else {
        out
    }
}

fn format_commands(kb: &KnowledgeBase) -> String {
    if kb.command_history.is_empty() {
        return "No commands recorded yet.".into();
    }
    kb.command_history
        .iter()
        .map(|c| {
            let status = match c.exit_code {
                Some(0) => "ok".to_string(),
                Some(code) => format!("exit:{code}"),
                None => "killed".to_string(),
            };
            let src = match c.source {
                crate::agent::knowledge::CommandSource::Terminal => "term",
                crate::agent::knowledge::CommandSource::Tool => "tool",
            };
            format!(
                "- [{}][{}] {} → {}",
                src,
                status,
                c.command,
                c.stdout.lines().next().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_notes(kb: &KnowledgeBase) -> String {
    if kb.notes.is_empty() {
        return "No notes recorded yet.".into();
    }
    kb.notes
        .iter()
        .map(|n| format!("- {n}"))
        .collect::<Vec<_>>()
        .join("\n")
}
