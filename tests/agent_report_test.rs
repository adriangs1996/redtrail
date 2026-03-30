use redtrail::core::analysis::analyze_session;
use redtrail::core::db::{self, AgentEvent, CommandFilter};

#[test]
fn agent_report_generates_analysis() {
    let conn = db::open_in_memory().unwrap();

    let agent_session_id = "test-agent-session-001";
    let session_id = db::find_or_create_agent_session(
        &conn,
        agent_session_id,
        Some("/tmp/test-project"),
        "claude_code",
    )
    .unwrap();

    // Seed several agent events
    db::insert_agent_event(&conn, &AgentEvent {
        session_id: session_id.clone(),
        command_raw: "Read src/main.rs".to_string(),
        command_binary: None,
        command_subcommand: None,
        command_args: None,
        command_flags: None,
        cwd: Some("/tmp/test-project".to_string()),
        git_repo: Some("/tmp/test-project".to_string()),
        git_branch: Some("main".to_string()),
        exit_code: Some(0),
        stdout: Some("fn main() {}".to_string()),
        stderr: None,
        stdout_truncated: false,
        stderr_truncated: false,
        source: "claude_code".to_string(),
        agent_session_id: Some(agent_session_id.to_string()),
        is_automated: true,
        redacted: false,
        tool_name: "Read".to_string(),
        tool_input: Some("src/main.rs".to_string()),
        tool_response: None,
    }).unwrap();

    db::insert_agent_event(&conn, &AgentEvent {
        session_id: session_id.clone(),
        command_raw: "Write src/lib.rs".to_string(),
        command_binary: None,
        command_subcommand: None,
        command_args: None,
        command_flags: None,
        cwd: Some("/tmp/test-project".to_string()),
        git_repo: Some("/tmp/test-project".to_string()),
        git_branch: Some("main".to_string()),
        exit_code: Some(0),
        stdout: None,
        stderr: None,
        stdout_truncated: false,
        stderr_truncated: false,
        source: "claude_code".to_string(),
        agent_session_id: Some(agent_session_id.to_string()),
        is_automated: true,
        redacted: false,
        tool_name: "Write".to_string(),
        tool_input: Some("src/lib.rs".to_string()),
        tool_response: None,
    }).unwrap();

    db::insert_agent_event(&conn, &AgentEvent {
        session_id: session_id.clone(),
        command_raw: "cargo test".to_string(),
        command_binary: Some("cargo".to_string()),
        command_subcommand: Some("test".to_string()),
        command_args: None,
        command_flags: None,
        cwd: Some("/tmp/test-project".to_string()),
        git_repo: Some("/tmp/test-project".to_string()),
        git_branch: Some("main".to_string()),
        exit_code: Some(1),
        stdout: Some("test result: FAILED".to_string()),
        stderr: Some("error[E0308]: mismatched types".to_string()),
        stdout_truncated: false,
        stderr_truncated: false,
        source: "claude_code".to_string(),
        agent_session_id: Some(agent_session_id.to_string()),
        is_automated: true,
        redacted: false,
        tool_name: "Bash".to_string(),
        tool_input: Some("cargo test".to_string()),
        tool_response: None,
    }).unwrap();

    // Fetch commands via the agent_session_id filter
    let commands = db::get_commands(&conn, &CommandFilter {
        agent_session_id: Some(agent_session_id),
        limit: Some(5000),
        ..Default::default()
    }).unwrap();

    assert_eq!(commands.len(), 3);

    let analysis = analyze_session(&commands);

    assert_eq!(analysis.total_commands, 3);
    assert_eq!(analysis.agent_commands, 0); // source is "claude_code", not "agent"
    assert!(analysis.total_errors >= 1);
    assert!(analysis.test_runs >= 1);
    assert!(analysis.tests_failed >= 1);
    assert!(analysis.files_created.contains(&"src/lib.rs".to_string()));
}

#[test]
fn agent_report_empty_session() {
    let conn = db::open_in_memory().unwrap();

    let commands = db::get_commands(&conn, &CommandFilter {
        agent_session_id: Some("nonexistent-session"),
        limit: Some(5000),
        ..Default::default()
    }).unwrap();

    assert!(commands.is_empty());
}
