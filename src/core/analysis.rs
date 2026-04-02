/// Session analysis: aggregate statistics from a slice of CommandRows.
use std::collections::{HashMap, HashSet};

use crate::core::classify::{CommandCategory, classify_command};
use crate::core::db::CommandRow;
use crate::core::errors::{ErrorFixSequence, detect_error_fix_sequences};

/// Per-binary execution statistics.
#[derive(Debug, Clone, Default)]
pub struct BinaryStats {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

/// Aggregated analysis of a session (or any slice of commands).
#[derive(Debug, Clone, Default)]
pub struct AnalysisResult {
    pub total_commands: usize,
    pub agent_commands: usize,
    pub human_commands: usize,

    /// Duration from first to last timestamp_start, in seconds.
    pub duration_seconds: i64,

    /// cwd from most recent command.
    pub directory: Option<String>,
    /// git_branch from most recent command.
    pub branch: Option<String>,
    /// source from most recent command.
    pub source: Option<String>,

    pub category_counts: HashMap<CommandCategory, usize>,
    pub binary_stats: HashMap<String, BinaryStats>,

    /// Files that were only written (never previously read in this session).
    pub files_created: Vec<String>,
    /// Files that were first read and then written.
    pub files_modified: Vec<String>,
    /// Files that were read but never written.
    pub files_read_only: Vec<String>,

    pub error_sequences: Vec<ErrorFixSequence>,

    pub test_runs: usize,
    pub tests_passed: usize,
    pub tests_failed: usize,

    pub total_errors: usize,
    pub errors_resolved: usize,
}

/// Binaries that read files (shell tools).
const FILE_READ_BINARIES: &[&str] = &["cat", "head", "tail", "less", "more", "bat", "wc"];

/// Tool names (Claude Code) that read files.
const FILE_READ_TOOLS: &[&str] = &["Read", "Glob", "Grep"];

/// Tool names (Claude Code) that write or modify files.
const FILE_WRITE_TOOLS: &[&str] = &["Write", "Edit", "NotebookEdit"];

/// Extract a file path from a tool-style command string.
///
/// Handles patterns like:
/// - "Read src/main.rs"
/// - "Edit src/lib.rs"
/// - "Write src/foo.rs"
/// - "cat README.md"
fn extract_file_path(command_raw: &str) -> Option<String> {
    let parts: Vec<&str> = command_raw.splitn(2, ' ').collect();
    if parts.len() == 2 {
        let path = parts[1].trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    None
}

/// Returns true if this command is a file-read operation.
fn is_file_read_command(row: &CommandRow) -> bool {
    if let Some(tool) = &row.tool_name
        && FILE_READ_TOOLS.contains(&tool.as_str())
    {
        return true;
    }
    row.command_binary
        .as_deref()
        .is_some_and(|b| FILE_READ_BINARIES.contains(&b))
}

/// Returns true if this command is a file-write operation.
fn is_file_write_command(row: &CommandRow) -> bool {
    row.tool_name
        .as_deref()
        .is_some_and(|t| FILE_WRITE_TOOLS.contains(&t))
}

/// Analyze a slice of commands and produce aggregated statistics.
pub fn analyze_session(commands: &[CommandRow]) -> AnalysisResult {
    if commands.is_empty() {
        return AnalysisResult::default();
    }

    // Sort by timestamp for deterministic analysis
    let mut sorted: Vec<&CommandRow> = commands.iter().collect();
    sorted.sort_by_key(|c| c.timestamp_start);

    let total_commands = sorted.len();
    let human_commands = sorted.iter().filter(|c| c.source == "human").count();
    let agent_commands = total_commands - human_commands;

    let first_ts = sorted.first().map(|c| c.timestamp_start).unwrap_or(0);
    let last_ts = sorted.last().map(|c| c.timestamp_start).unwrap_or(0);
    let duration_seconds = last_ts.saturating_sub(first_ts);

    // Context from most recent command
    let most_recent = sorted.last().unwrap();
    let directory = most_recent.cwd.clone();
    let branch = most_recent.git_branch.clone();
    let source = Some(most_recent.source.clone());

    // Category counts and binary stats
    let mut category_counts: HashMap<CommandCategory, usize> = HashMap::new();
    let mut binary_stats: HashMap<String, BinaryStats> = HashMap::new();

    // Test run tracking
    let mut test_runs = 0usize;
    let mut tests_passed = 0usize;
    let mut tests_failed = 0usize;

    // Error tracking
    let mut total_errors = 0usize;

    // File tracking: order matters — we walk in timestamp order
    let mut files_read: HashSet<String> = HashSet::new();
    let mut files_written: HashSet<String> = HashSet::new();

    for cmd in &sorted {
        let binary = cmd.command_binary.as_deref().unwrap_or("");
        let subcommand = cmd.command_subcommand.as_deref();
        let tool = cmd.tool_name.as_deref();

        let category = classify_command(binary, subcommand, tool);
        *category_counts.entry(category).or_insert(0) += 1;

        // Binary stats (skip empty binary)
        if !binary.is_empty() {
            let stats = binary_stats.entry(binary.to_string()).or_default();
            stats.total += 1;
            match cmd.exit_code {
                Some(0) => stats.succeeded += 1,
                Some(_) => stats.failed += 1,
                None => {}
            }
        }

        // Error detection
        if cmd.exit_code.is_some_and(|c| c != 0) {
            total_errors += 1;
        }

        // Test run tracking
        if category == CommandCategory::TestRun {
            test_runs += 1;
            match cmd.exit_code {
                Some(0) => tests_passed += 1,
                Some(_) => tests_failed += 1,
                None => {}
            }
        }

        // File reads
        if is_file_read_command(cmd)
            && let Some(path) = extract_file_path(&cmd.command_raw)
        {
            files_read.insert(path);
        }

        // File writes
        if is_file_write_command(cmd)
            && let Some(path) = extract_file_path(&cmd.command_raw)
        {
            files_written.insert(path);
        }
    }

    // Build file categorization lists
    let mut files_modified: Vec<String> = files_written
        .iter()
        .filter(|p| files_read.contains(*p))
        .cloned()
        .collect();
    files_modified.sort();

    let mut files_created: Vec<String> = files_written
        .iter()
        .filter(|p| !files_read.contains(*p))
        .cloned()
        .collect();
    files_created.sort();

    let mut files_read_only: Vec<String> = files_read
        .iter()
        .filter(|p| !files_written.contains(*p))
        .cloned()
        .collect();
    files_read_only.sort();

    // Error-fix sequences
    let error_sequences = detect_error_fix_sequences(commands);
    let errors_resolved = error_sequences.iter().filter(|s| s.resolved).count();

    AnalysisResult {
        total_commands,
        agent_commands,
        human_commands,
        duration_seconds,
        directory,
        branch,
        source,
        category_counts,
        binary_stats,
        files_created,
        files_modified,
        files_read_only,
        error_sequences,
        test_runs,
        tests_passed,
        tests_failed,
        total_errors,
        errors_resolved,
    }
}
