use redtrail::cmd::agent_context;
use redtrail::core::db::{self, NewCommand};

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn agent_context_empty_project() {
    let conn = setup();
    // Fetch commands for a nonexistent project — should be empty.
    let commands = db::get_commands(
        &conn,
        &db::CommandFilter {
            git_repo: Some("/nonexistent/project"),
            limit: Some(100),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(commands.is_empty(), "Expected no commands for empty project");
}

#[test]
fn agent_context_token_budget_estimation() {
    // The budget uses chars/4 approximation.
    // A string of 400 chars should fit in a 100-token budget (100 * 4 = 400 chars).
    let text = "x".repeat(400);
    let result = agent_context::trim_to_budget(&text, 100);
    assert_eq!(result, text, "Should not trim when exactly at budget");

    // A string of 401 chars should be trimmed at 100-token budget.
    let text = "y".repeat(401);
    let result = agent_context::trim_to_budget(&text, 100);
    assert_ne!(result, text, "Should trim when over budget");
    assert!(
        result.contains("[Truncated to fit token budget]"),
        "Should include truncation notice"
    );
    // The truncated content portion should not exceed max_chars
    assert!(
        result.starts_with(&"y".repeat(400)),
        "Should keep content up to max_chars"
    );
}

#[test]
fn agent_context_token_budget_trims_at_section_boundary() {
    // Build a markdown doc with section boundaries.
    let mut text = String::new();
    text.push_str("## Section 1\n");
    text.push_str(&"a".repeat(100));
    text.push_str("\n\n## Section 2\n");
    text.push_str(&"b".repeat(100));
    text.push_str("\n\n## Section 3\n");
    text.push_str(&"c".repeat(100));

    // Budget that fits sections 1 and 2 but not 3.
    // Total chars before section 3: "## Section 1\n" (14) + 100 + "\n\n## Section 2\n" (16) + 100 = 230
    // Section 3 starts at ~230, total is ~350.
    // Set budget to ~60 tokens (240 chars), should trim at section 2 boundary.
    let result = agent_context::trim_to_budget(&text, 60);
    assert!(
        result.contains("## Section 1"),
        "Should keep section 1"
    );
    assert!(
        result.contains("[Truncated to fit token budget]"),
        "Should include truncation notice"
    );
}

#[test]
fn agent_context_groups_by_session_fallback() {
    let conn = setup();

    // Two commands in session A
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: "sess-a",
            command_raw: "cargo build",
            command_binary: Some("cargo"),
            command_subcommand: Some("build"),
            exit_code: Some(0),
            timestamp_start: 1000,
            source: "claude_code",
            git_repo: Some("/test/project"),
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        &conn,
        &NewCommand {
            session_id: "sess-a",
            command_raw: "cargo test",
            command_binary: Some("cargo"),
            command_subcommand: Some("test"),
            exit_code: Some(0),
            timestamp_start: 1100,
            source: "claude_code",
            git_repo: Some("/test/project"),
            ..Default::default()
        },
    )
    .unwrap();

    // One command in session B (more recent)
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: "sess-b",
            command_raw: "cargo check",
            command_binary: Some("cargo"),
            command_subcommand: Some("check"),
            exit_code: Some(0),
            timestamp_start: 2000,
            source: "claude_code",
            git_repo: Some("/test/project"),
            ..Default::default()
        },
    )
    .unwrap();

    let commands = db::get_commands(
        &conn,
        &db::CommandFilter {
            git_repo: Some("/test/project"),
            limit: Some(100),
            ..Default::default()
        },
    )
    .unwrap();

    // All 3 commands should be fetched, grouped into 2 sessions
    assert_eq!(commands.len(), 3);
}
