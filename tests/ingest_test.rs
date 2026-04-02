use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// --- Schema migration ---

#[test]
fn fresh_db_has_tool_columns() {
    let conn = setup();
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(commands)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(columns.contains(&"tool_name".to_string()));
    assert!(columns.contains(&"tool_input".to_string()));
    assert!(columns.contains(&"tool_response".to_string()));
}

#[test]
fn tool_index_exists() {
    let conn = setup();
    let exists: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type='index' AND name='idx_commands_tool'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(exists);
}

#[test]
fn busy_timeout_is_set() {
    let conn = setup();
    let timeout: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(timeout, 3000);
}

// --- Agent event insert ---

fn make_event(session_id: &str) -> db::AgentEvent {
    db::AgentEvent {
        session_id: session_id.to_string(),
        command_raw: "git push origin main".into(),
        command_binary: Some("git".into()),
        command_subcommand: Some("push".into()),
        command_args: Some(r#"["origin","main"]"#.into()),
        command_flags: Some("{}".into()),
        cwd: Some("/home/user/project".into()),
        git_repo: Some("/home/user/project".into()),
        git_branch: Some("main".into()),
        exit_code: Some(0),
        stdout: Some("Everything up-to-date".into()),
        stderr: None,
        stdout_truncated: false,
        stderr_truncated: false,
        source: "claude_code".into(),
        agent_session_id: Some("claude-session-123".into()),
        is_automated: true,
        redacted: false,
        tool_name: "Bash".into(),
        tool_input: Some(r#"{"command":"git push origin main"}"#.into()),
        tool_response: Some(
            r#"{"stdout":"Everything up-to-date","stderr":"","exitCode":0}"#.into(),
        ),
    }
}

#[test]
fn insert_agent_event_stores_and_retrieves() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "claude_code",
            ..Default::default()
        },
    )
    .unwrap();

    let id = db::insert_agent_event(&conn, &make_event(&session_id)).unwrap();
    assert!(!id.is_empty());

    // Verify stored data
    let (tool_name, tool_input, source, is_automated): (
        Option<String>,
        Option<String>,
        String,
        bool,
    ) = conn
        .query_row(
            "SELECT tool_name, tool_input, source, is_automated FROM commands WHERE id = ?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(tool_name.as_deref(), Some("Bash"));
    assert!(tool_input.unwrap().contains("git push"));
    assert_eq!(source, "claude_code");
    assert!(is_automated);
}

#[test]
fn insert_agent_event_increments_session_counters() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "claude_code",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_agent_event(&conn, &make_event(&session_id)).unwrap();

    let (cmd_count, agent_count): (i64, i64) = conn
        .query_row(
            "SELECT command_count, agent_command_count FROM sessions WHERE id = ?1",
            [&session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(cmd_count, 1);
    assert_eq!(agent_count, 1);
}

#[test]
fn insert_agent_event_error_increments_error_count() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "claude_code",
            ..Default::default()
        },
    )
    .unwrap();

    let mut evt = make_event(&session_id);
    evt.exit_code = Some(1);
    db::insert_agent_event(&conn, &evt).unwrap();

    let error_count: i64 = conn
        .query_row(
            "SELECT error_count FROM sessions WHERE id = ?1",
            [&session_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(error_count, 1);
}

#[test]
fn insert_agent_event_syncs_fts() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "claude_code",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_agent_event(&conn, &make_event(&session_id)).unwrap();

    // FTS should find the command
    let results = db::search_commands(&conn, "\"up-to-date\"", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source, "claude_code");
}

#[test]
fn agent_event_null_exit_code_for_non_bash() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "claude_code",
            ..Default::default()
        },
    )
    .unwrap();

    let mut evt = make_event(&session_id);
    evt.tool_name = "Edit".into();
    evt.command_raw = "Edit src/main.rs".into();
    evt.command_binary = Some("Edit".into());
    evt.exit_code = None;
    evt.stdout = Some("file edited".into());
    evt.stderr = None;

    db::insert_agent_event(&conn, &evt).unwrap();

    let filter = db::CommandFilter {
        tool_name: Some("Edit"),
        ..Default::default()
    };
    let results = db::get_commands(&conn, &filter).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].exit_code.is_none());
}

// --- find_or_create_agent_session ---

#[test]
fn find_or_create_creates_new_session() {
    let conn = setup();
    let id = db::find_or_create_agent_session(
        &conn,
        "claude-abc-123",
        Some("/home/user"),
        "claude_code",
    )
    .unwrap();

    assert!(!id.is_empty());

    let session = db::get_session(&conn, &id).unwrap();
    assert_eq!(session.source, "claude_code");
    assert_eq!(session.cwd_initial.as_deref(), Some("/home/user"));
}

#[test]
fn find_or_create_reuses_existing_session() {
    let conn = setup();
    let id1 = db::find_or_create_agent_session(
        &conn,
        "claude-abc-123",
        Some("/home/user"),
        "claude_code",
    )
    .unwrap();

    let id2 = db::find_or_create_agent_session(
        &conn,
        "claude-abc-123",
        Some("/different/dir"),
        "claude_code",
    )
    .unwrap();

    assert_eq!(id1, id2);
}

#[test]
fn different_agent_sessions_get_different_redtrail_sessions() {
    let conn = setup();
    let id1 =
        db::find_or_create_agent_session(&conn, "claude-session-1", None, "claude_code").unwrap();

    let id2 =
        db::find_or_create_agent_session(&conn, "claude-session-2", None, "claude_code").unwrap();

    assert_ne!(id1, id2);
}

// --- Filter by source and tool ---

#[test]
fn filter_by_source() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Insert a human command
    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: &session_id,
            command_raw: "ls -la",
            source: "human",
            timestamp_start: now_secs(),
            ..Default::default()
        },
    )
    .unwrap();

    // Insert an agent event
    db::insert_agent_event(&conn, &make_event(&session_id)).unwrap();

    // Filter human only
    let human = db::get_commands(
        &conn,
        &db::CommandFilter {
            source: Some("human"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(human.len(), 1);
    assert_eq!(human[0].source, "human");

    // Filter agent only
    let agent = db::get_commands(
        &conn,
        &db::CommandFilter {
            source: Some("claude_code"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(agent.len(), 1);
    assert_eq!(agent[0].source, "claude_code");
}

#[test]
fn filter_by_tool_name() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "claude_code",
            ..Default::default()
        },
    )
    .unwrap();

    // Insert Bash event
    db::insert_agent_event(&conn, &make_event(&session_id)).unwrap();

    // Insert Edit event
    let mut edit_evt = make_event(&session_id);
    edit_evt.tool_name = "Edit".into();
    edit_evt.command_raw = "Edit src/main.rs".into();
    edit_evt.command_binary = Some("Edit".into());
    edit_evt.exit_code = None;
    db::insert_agent_event(&conn, &edit_evt).unwrap();

    let bash_only = db::get_commands(
        &conn,
        &db::CommandFilter {
            tool_name: Some("Bash"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bash_only.len(), 1);
    assert!(bash_only[0].command_raw.contains("git push"));

    let edit_only = db::get_commands(
        &conn,
        &db::CommandFilter {
            tool_name: Some("Edit"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(edit_only.len(), 1);
    assert!(edit_only[0].command_raw.contains("Edit"));
}

// --- Human commands have null tool_name ---

#[test]
fn human_commands_have_null_tool_name() {
    let conn = setup();
    let session_id = db::create_session(
        &conn,
        &db::NewSession {
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: &session_id,
            command_raw: "ls -la",
            source: "human",
            timestamp_start: now_secs(),
            ..Default::default()
        },
    )
    .unwrap();

    let tool_name: Option<String> = conn
        .query_row(
            "SELECT tool_name FROM commands WHERE command_raw = 'ls -la'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(tool_name.is_none());
}

// --- Lifecycle event ingestion via run_from_reader ---

use redtrail::cmd::ingest::run_from_reader;
use std::io::Cursor;

fn ingest_lifecycle(conn: &rusqlite::Connection, event_type: &str, json: &str) {
    run_from_reader(conn, event_type, Cursor::new(json.as_bytes().to_vec())).unwrap();
}

#[test]
fn ingest_subagent_start_stores_correctly() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","agent_id":"a-123","agent_type":"Explore"}"#;
    ingest_lifecycle(&conn, "SubagentStart", json);

    let (tool_name, command_raw, command_binary): (String, String, Option<String>) = conn
        .query_row(
            "SELECT tool_name, command_raw, command_binary FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    assert_eq!(tool_name, "SubagentStart");
    assert_eq!(command_raw, "Agent Explore started");
    assert_eq!(command_binary.as_deref(), Some("Agent"));
}

#[test]
fn ingest_subagent_stop_stores_last_message_as_stdout() {
    let conn = setup();
    let json = r#"{
        "session_id": "sess-1",
        "agent_id": "a-123",
        "agent_type": "Plan",
        "agent_transcript_path": "/tmp/transcript.jsonl",
        "last_assistant_message": "Here is my plan for the implementation."
    }"#;
    ingest_lifecycle(&conn, "SubagentStop", json);

    let (tool_name, stdout): (String, Option<String>) = conn
        .query_row(
            "SELECT tool_name, stdout FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(tool_name, "SubagentStop");
    assert_eq!(
        stdout.as_deref(),
        Some("Here is my plan for the implementation.")
    );
}

#[test]
fn ingest_user_prompt_stores_prompt_as_command_raw() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","prompt":"Fix the authentication bug in login.rs"}"#;
    ingest_lifecycle(&conn, "UserPromptSubmit", json);

    let command_raw: String = conn
        .query_row(
            "SELECT command_raw FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(command_raw, "Fix the authentication bug in login.rs");

    // Verify FTS indexes the prompt
    let results = db::search_commands(&conn, "authentication", 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn ingest_user_prompt_redacts_secrets() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","prompt":"Use this key: AKIAIOSFODNN7EXAMPLE to deploy"}"#;
    ingest_lifecycle(&conn, "UserPromptSubmit", json);

    let (command_raw, redacted): (String, bool) = conn
        .query_row(
            "SELECT command_raw, redacted FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert!(redacted, "should be marked as redacted");
    assert!(
        !command_raw.contains("AKIAIOSFODNN7EXAMPLE"),
        "secret should be redacted from command_raw"
    );
    assert!(
        command_raw.contains("[REDACTED"),
        "should contain redaction marker"
    );
}

#[test]
fn ingest_session_start_stores_source_and_model() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","source":"startup","model":"claude-sonnet-4-6"}"#;
    ingest_lifecycle(&conn, "SessionStart", json);

    let (tool_name, command_raw, tool_input): (String, String, Option<String>) = conn
        .query_row(
            "SELECT tool_name, command_raw, tool_input FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    assert_eq!(tool_name, "SessionStart");
    assert!(command_raw.contains("startup"));
    assert!(command_raw.contains("claude-sonnet-4-6"));
    let ti = tool_input.unwrap();
    assert!(ti.contains("startup"));
    assert!(ti.contains("claude-sonnet-4-6"));
}

#[test]
fn ingest_session_end_stores_minimal_event() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1"}"#;
    ingest_lifecycle(&conn, "SessionEnd", json);

    let (tool_name, command_raw): (String, String) = conn
        .query_row(
            "SELECT tool_name, command_raw FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(tool_name, "SessionEnd");
    assert_eq!(command_raw, "Session ended");
}

#[test]
fn ingest_stop_stores_reason() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","stop_reason":"end_turn"}"#;
    ingest_lifecycle(&conn, "Stop", json);

    let command_raw: String = conn
        .query_row(
            "SELECT command_raw FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert!(command_raw.contains("end_turn"));
}

#[test]
fn ingest_instructions_loaded_stores_path() {
    let conn = setup();
    let json = r#"{
        "session_id": "sess-1",
        "file_path": "/Users/dev/project/CLAUDE.md",
        "memory_type": "Project",
        "load_reason": "session_start"
    }"#;
    ingest_lifecycle(&conn, "InstructionsLoaded", json);

    let (tool_name, command_raw): (String, String) = conn
        .query_row(
            "SELECT tool_name, command_raw FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(tool_name, "InstructionsLoaded");
    assert!(command_raw.contains("CLAUDE.md"));
    assert!(command_raw.contains("session_start"));
}

#[test]
fn ingest_config_change_stores_event() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","config_source":"skills"}"#;
    ingest_lifecycle(&conn, "ConfigChange", json);

    let tool_name: String = conn
        .query_row(
            "SELECT tool_name FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(tool_name, "ConfigChange");
}

#[test]
fn ingest_unknown_event_returns_error() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1"}"#;
    let result = run_from_reader(&conn, "UnknownEvent", Cursor::new(json.as_bytes().to_vec()));
    assert!(result.is_err());
}

#[test]
fn ingest_backward_compat_default_post_tool_use() {
    let conn = setup();
    let json = r#"{
        "session_id": "sess-1",
        "tool_name": "Bash",
        "tool_input": {"command": "echo hello"},
        "tool_response": {"stdout": "hello", "stderr": "", "exitCode": 0},
        "cwd": "/tmp"
    }"#;
    // Default event type is PostToolUse
    run_from_reader(&conn, "PostToolUse", Cursor::new(json.as_bytes().to_vec())).unwrap();

    let tool_name: String = conn
        .query_row(
            "SELECT tool_name FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(tool_name, "Bash");
}

#[test]
fn ingest_skill_tool_derives_clean_command_raw() {
    let conn = setup();
    let json = r#"{
        "session_id": "sess-1",
        "tool_name": "Skill",
        "tool_input": {"skill": "commit", "args": "-m 'fix bug'"},
        "tool_response": {"success": true},
        "cwd": "/tmp"
    }"#;
    run_from_reader(&conn, "PostToolUse", Cursor::new(json.as_bytes().to_vec())).unwrap();

    let command_raw: String = conn
        .query_row(
            "SELECT command_raw FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(command_raw, "Skill commit");
}

#[test]
fn ingest_user_prompt_with_slash_skill() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","prompt":"/commit -m 'initial'"}"#;
    ingest_lifecycle(&conn, "UserPromptSubmit", json);

    let command_raw: String = conn
        .query_row(
            "SELECT command_raw FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(command_raw, "/commit -m 'initial'");
}

#[test]
fn lifecycle_events_are_automated_and_claude_code_source() {
    let conn = setup();
    let json = r#"{"session_id":"sess-1","prompt":"hello"}"#;
    ingest_lifecycle(&conn, "UserPromptSubmit", json);

    let (source, is_automated): (String, bool) = conn
        .query_row(
            "SELECT source, is_automated FROM commands ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(source, "claude_code");
    assert!(is_automated);
}
