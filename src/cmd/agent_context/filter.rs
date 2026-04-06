// Command noise filtering and formatting for agent-context output.

use crate::core::classify::{classify_command, is_noise_command, CommandCategory};
use crate::core::db::CommandRow;
use crate::core::fmt::paths::to_relative;

/// Filter commands to only meaningful ones (writes, builds, tests, git, failures).
pub fn filter_meaningful(commands: &[CommandRow]) -> Vec<&CommandRow> {
    commands
        .iter()
        .filter(|cmd| {
            // Always keep failed commands on project binaries
            if cmd.exit_code.is_some_and(|c| c != 0) {
                return true;
            }
            !is_noise_command(cmd)
        })
        .collect()
}

/// Return the last N meaningful commands sorted by timestamp ascending.
pub fn last_meaningful(commands: &[CommandRow], n: usize) -> Vec<&CommandRow> {
    let mut meaningful = filter_meaningful(commands);
    meaningful.sort_by_key(|c| c.timestamp_start);
    let skip = meaningful.len().saturating_sub(n);
    meaningful.into_iter().skip(skip).collect()
}

/// Format a command as a single line with status icon and relative paths.
/// Icons: ~ (write/edit), ✓ (success), ✗ (failed), · (no exit code)
pub fn format_command_line(cmd: &CommandRow, project_root: &str) -> String {
    let icon = command_icon(cmd);
    let label = command_label(cmd, project_root);
    format!("{icon} {label}")
}

fn command_icon(cmd: &CommandRow) -> &'static str {
    let category = classify_command(
        cmd.command_binary.as_deref().unwrap_or(""),
        cmd.command_subcommand.as_deref(),
        cmd.tool_name.as_deref(),
    );
    if category == CommandCategory::FileWrite {
        return "~";
    }
    match cmd.exit_code {
        Some(0) => "✓",
        Some(_) => "✗",
        None => "·",
    }
}

fn command_label(cmd: &CommandRow, project_root: &str) -> String {
    if let Some(tool) = &cmd.tool_name {
        match tool.as_str() {
            "Write" | "Edit" | "NotebookEdit" => {
                let path = extract_tool_path(&cmd.command_raw);
                let rel = to_relative(&path, project_root);
                return format!("{tool} {rel}");
            }
            "Bash" => {
                return truncate_with_relative_paths(&cmd.command_raw, project_root, 80);
            }
            _ => {}
        }
    }
    truncate_with_relative_paths(&cmd.command_raw, project_root, 80)
}

fn extract_tool_path(command_raw: &str) -> String {
    command_raw
        .split_once(' ')
        .map(|(_, rest)| rest)
        .unwrap_or(command_raw)
        .trim()
        .to_string()
}

fn truncate_with_relative_paths(cmd: &str, project_root: &str, max_len: usize) -> String {
    let replaced = cmd.replace(project_root, ".");
    let replaced = replaced.replace(&format!("{}/", project_root.trim_end_matches('/')), "");
    if replaced.len() <= max_len {
        replaced
    } else {
        format!("{}...", &replaced[..max_len.saturating_sub(3)])
    }
}
