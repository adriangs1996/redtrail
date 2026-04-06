use redtrail::config::{LlmConfig, OllamaConfig};
use redtrail::core::db;
use redtrail::extract;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn insert_unstructured_command(conn: &rusqlite::Connection, id: &str, stdout: &str) {
    let _ = conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    );
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, stdout, source, status)
         VALUES (?1, 'sess', 1000, 'npm run build', 'npm', ?2, 'human', 'finished')",
        rusqlite::params![id, stdout],
    )
    .unwrap();
}

fn insert_git_command(conn: &rusqlite::Connection, id: &str) {
    let _ = conn.execute(
        "INSERT OR IGNORE INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    );
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, command_subcommand, stdout, git_repo, source, status)
         VALUES (?1, 'sess', 1000, 'git status', 'git', 'status', ' M src/main.rs\n', '/repo', 'human', 'finished')",
        rusqlite::params![id],
    )
    .unwrap();
}

fn llm_config_unreachable() -> LlmConfig {
    LlmConfig {
        enabled: true,
        provider: "ollama".to_string(),
        ollama: OllamaConfig {
            url: "http://127.0.0.1:19999".to_string(),
            model: "gemma4".to_string(),
        },
        timeout_seconds: 2,
        max_input_chars: 4096,
    }
}

#[test]
fn llm_fallback_graceful_when_unavailable() {
    let conn = setup();
    // Unstructured output that generic extractor won't parse much from
    insert_unstructured_command(&conn, "cmd-1", "Build failed: some internal error occurred\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    let cfg = llm_config_unreachable();

    // Should succeed even though Ollama is unreachable
    let result = extract::extract_command(&conn, &cmd, Some(&cfg));
    assert!(result.is_ok(), "extraction must not fail when LLM is unreachable");

    // Command should be marked as extracted (skipped or generic, not "llm")
    let method: String = conn
        .query_row(
            "SELECT extraction_method FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(method, "llm", "method should not be 'llm' when Ollama is down");
}

#[test]
fn llm_not_called_when_heuristics_succeed() {
    let conn = setup();
    insert_git_command(&conn, "cmd-1");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    let cfg = llm_config_unreachable();

    extract::extract_command(&conn, &cmd, Some(&cfg)).unwrap();

    // Git heuristic should succeed — method should be "heuristic", not "llm"
    let method: String = conn
        .query_row(
            "SELECT extraction_method FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(method, "heuristic", "domain extractor should take precedence over LLM");
}

#[test]
fn llm_not_called_when_disabled() {
    let conn = setup();
    insert_unstructured_command(&conn, "cmd-1", "some output\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    let mut cfg = llm_config_unreachable();
    cfg.enabled = false;

    extract::extract_command(&conn, &cmd, Some(&cfg)).unwrap();

    let extracted: bool = conn
        .query_row(
            "SELECT extracted FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(extracted, "command should still be marked as extracted");
}

#[test]
fn llm_skipped_when_config_is_none() {
    let conn = setup();
    insert_unstructured_command(&conn, "cmd-1", "some output\n");

    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();

    // None config = no LLM, should still work fine
    extract::extract_command(&conn, &cmd, None).unwrap();

    let extracted: bool = conn
        .query_row(
            "SELECT extracted FROM commands WHERE id = 'cmd-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(extracted);
}
