#[derive(Default, Clone, Copy)]
pub struct NewCommand<'a> {
    pub session_id: &'a str,
    pub command_raw: &'a str,
    pub command_binary: Option<&'a str>,
    pub command_subcommand: Option<&'a str>,
    pub command_args: Option<&'a str>,
    pub command_flags: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub git_repo: Option<&'a str>,
    pub git_branch: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub stdout: Option<&'a str>,
    pub stderr: Option<&'a str>,
    pub env_snapshot: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub source: &'a str,
    pub timestamp_start: i64,
    pub timestamp_end: Option<i64>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub redacted: bool,
}

#[derive(Clone, Default)]
pub struct CommandRow {
    pub id: String,
    pub session_id: String,
    pub command_raw: String,
    pub command_binary: Option<String>,
    pub cwd: Option<String>,
    pub exit_code: Option<i32>,
    pub hostname: Option<String>,
    pub shell: Option<String>,
    pub source: String,
    pub timestamp_start: i64,
    pub timestamp_end: Option<i64>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub redacted: bool,
    pub tool_name: Option<String>,
    pub command_subcommand: Option<String>,
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
    pub agent_session_id: Option<String>,
}

#[derive(Default)]
pub struct CommandFilter<'a> {
    pub failed_only: bool,
    pub command_binary: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub since: Option<i64>,
    pub limit: Option<usize>,
    pub source: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub agent_session_id: Option<&'a str>,
    pub git_repo: Option<&'a str>,
}

/// Minimal input needed to record that a command has started.
#[derive(Default)]
pub struct NewCommandStart<'a> {
    pub session_id: &'a str,
    pub command_raw: &'a str,
    pub command_binary: Option<&'a str>,
    pub command_subcommand: Option<&'a str>,
    pub command_args: Option<&'a str>,
    pub command_flags: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub source: &'a str,
    pub redacted: bool,
}

/// Fields needed to close out a running command.
pub struct FinishCommand<'a> {
    pub command_id: &'a str,
    pub exit_code: Option<i32>,
    pub git_repo: Option<&'a str>,
    pub git_branch: Option<&'a str>,
    pub env_snapshot: Option<&'a str>,
    /// Final stdout — merged with any in-progress stdout via COALESCE so streaming
    /// output written by `update_command_output` is preserved when this is None.
    pub stdout: Option<&'a str>,
    /// Final stderr — same COALESCE semantics as stdout.
    pub stderr: Option<&'a str>,
}

#[derive(Default)]
pub struct NewSession<'a> {
    pub cwd_initial: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub source: &'a str,
}

pub struct SessionRow {
    pub id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub cwd_initial: Option<String>,
    pub hostname: Option<String>,
    pub shell: Option<String>,
    pub source: String,
    pub command_count: i64,
    pub error_count: i64,
}

pub struct RedactionLogEntry {
    pub field: String,
    pub pattern_label: String,
    pub redacted_at: i64,
}

pub struct AgentEvent {
    pub session_id: String,
    pub command_raw: String,
    pub command_binary: Option<String>,
    pub command_subcommand: Option<String>,
    pub command_args: Option<String>,
    pub command_flags: Option<String>,
    pub cwd: Option<String>,
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub source: String,
    pub agent_session_id: Option<String>,
    pub is_automated: bool,
    pub redacted: bool,
    pub tool_name: String,
    pub tool_input: Option<String>,
    pub tool_response: Option<String>,
}
