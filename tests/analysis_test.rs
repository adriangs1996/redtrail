use redtrail::core::analysis::analyze_session;
use redtrail::core::classify::CommandCategory;
use redtrail::core::db::CommandRow;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_row(
    session_id: &str,
    command_raw: &str,
    command_binary: Option<&str>,
    command_subcommand: Option<&str>,
    tool_name: Option<&str>,
    exit_code: Option<i32>,
    timestamp_start: i64,
    source: &str,
) -> CommandRow {
    CommandRow {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        command_raw: command_raw.to_string(),
        command_binary: command_binary.map(str::to_string),
        command_subcommand: command_subcommand.map(str::to_string),
        tool_name: tool_name.map(str::to_string),
        exit_code,
        timestamp_start,
        source: source.to_string(),
        ..Default::default()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn analyze_empty_session() {
    let result = analyze_session(&[]);
    assert_eq!(result.total_commands, 0);
    assert_eq!(result.agent_commands, 0);
    assert_eq!(result.human_commands, 0);
    assert_eq!(result.duration_seconds, 0);
    assert!(result.directory.is_none());
    assert!(result.branch.is_none());
    assert!(result.files_created.is_empty());
    assert!(result.files_modified.is_empty());
    assert!(result.files_read_only.is_empty());
    assert!(result.error_sequences.is_empty());
    assert_eq!(result.test_runs, 0);
    assert_eq!(result.tests_passed, 0);
    assert_eq!(result.tests_failed, 0);
    assert_eq!(result.total_errors, 0);
    assert_eq!(result.errors_resolved, 0);
}

#[test]
fn analyze_counts_by_category() {
    let commands = vec![
        make_row(
            "sess-1",
            "git status",
            Some("git"),
            Some("status"),
            None,
            Some(0),
            1000,
            "human",
        ),
        make_row(
            "sess-1",
            "cargo test",
            Some("cargo"),
            Some("test"),
            None,
            Some(0),
            1010,
            "human",
        ),
    ];

    let result = analyze_session(&commands);

    assert_eq!(result.total_commands, 2);
    assert_eq!(result.human_commands, 2);
    assert_eq!(result.agent_commands, 0);

    let git_count = result
        .category_counts
        .get(&CommandCategory::GitOperation)
        .copied()
        .unwrap_or(0);
    assert_eq!(git_count, 1, "expected 1 git operation");

    let test_count = result
        .category_counts
        .get(&CommandCategory::TestRun)
        .copied()
        .unwrap_or(0);
    assert_eq!(test_count, 1, "expected 1 test run");

    assert_eq!(result.test_runs, 1);
    assert_eq!(result.tests_passed, 1);
    assert_eq!(result.tests_failed, 0);
}

#[test]
fn analyze_extracts_file_lists() {
    // Read "src/lib.rs"  (read-only, never written)
    // Write "src/new.rs" (created — not previously read)
    // Read then Edit "src/main.rs" (modified — was read before written)

    let commands = vec![
        make_row(
            "sess-2",
            "Read src/lib.rs",
            Some("Read"),
            None,
            Some("Read"),
            Some(0),
            2000,
            "agent",
        ),
        make_row(
            "sess-2",
            "Write src/new.rs",
            Some("Write"),
            None,
            Some("Write"),
            Some(0),
            2010,
            "agent",
        ),
        make_row(
            "sess-2",
            "Read src/main.rs",
            Some("Read"),
            None,
            Some("Read"),
            Some(0),
            2020,
            "agent",
        ),
        make_row(
            "sess-2",
            "Edit src/main.rs",
            Some("Edit"),
            None,
            Some("Edit"),
            Some(0),
            2030,
            "agent",
        ),
    ];

    let result = analyze_session(&commands);

    assert_eq!(result.agent_commands, 4);

    assert!(
        result.files_created.contains(&"src/new.rs".to_string()),
        "expected src/new.rs in files_created, got: {:?}",
        result.files_created
    );

    assert!(
        result.files_modified.contains(&"src/main.rs".to_string()),
        "expected src/main.rs in files_modified, got: {:?}",
        result.files_modified
    );

    assert!(
        result.files_read_only.contains(&"src/lib.rs".to_string()),
        "expected src/lib.rs in files_read_only, got: {:?}",
        result.files_read_only
    );

    // Sanity: nothing should appear in the wrong bucket
    assert!(
        !result.files_modified.contains(&"src/lib.rs".to_string()),
        "src/lib.rs should not be modified"
    );
    assert!(
        !result.files_created.contains(&"src/main.rs".to_string()),
        "src/main.rs should not be created"
    );
}

#[test]
fn analyze_computes_duration() {
    let commands = vec![
        make_row(
            "sess-3",
            "git status",
            Some("git"),
            Some("status"),
            None,
            Some(0),
            1000,
            "human",
        ),
        make_row(
            "sess-3",
            "cargo build",
            Some("cargo"),
            Some("build"),
            None,
            Some(0),
            2000,
            "human",
        ),
    ];

    let result = analyze_session(&commands);

    assert_eq!(
        result.duration_seconds, 1000,
        "expected duration 1000s, got {}",
        result.duration_seconds
    );
}
