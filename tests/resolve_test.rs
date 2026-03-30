use redtrail::core::db::{self, CommandFilter, NewCommand};
use redtrail::core::errors::normalize_error;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

// ── FTS search tests ────────────────────────────────────────────────────────

#[test]
fn resolve_finds_matching_errors_via_fts() {
    let conn = setup();
    let session = "sess-resolve-1";

    // Seed a failed command with a recognizable error
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "npm test",
            command_binary: Some("npm"),
            command_subcommand: Some("test"),
            exit_code: Some(1),
            stderr: Some("Error: Cannot find module 'bcrypt'"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Seed the resolution: npm install bcrypt succeeds
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "npm install bcrypt",
            command_binary: Some("npm"),
            command_subcommand: Some("install"),
            exit_code: Some(0),
            timestamp_start: 1100,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Then npm test succeeds (this is the resolution for the failed npm test)
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "npm test",
            command_binary: Some("npm"),
            command_subcommand: Some("test"),
            exit_code: Some(0),
            timestamp_start: 1200,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Use FTS search to find the error
    let results = db::search_commands(&conn, "Cannot find module bcrypt", 50).unwrap();
    assert!(
        !results.is_empty(),
        "FTS should find commands matching the error"
    );

    // The failed command should be in results
    let failed: Vec<_> = results
        .iter()
        .filter(|c| c.exit_code.is_some_and(|code| code != 0))
        .collect();
    assert!(
        !failed.is_empty(),
        "Should find at least one failed command"
    );
    assert_eq!(failed[0].command_raw, "npm test");
    assert!(
        failed[0]
            .stderr
            .as_deref()
            .unwrap()
            .contains("Cannot find module"),
        "stderr should contain the error"
    );
}

#[test]
fn resolve_finds_resolution_within_time_window() {
    let conn = setup();
    let session = "sess-resolve-2";

    // Failed cargo build
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo build",
            command_binary: Some("cargo"),
            command_subcommand: Some("build"),
            exit_code: Some(1),
            stderr: Some("error[E0308]: mismatched types"),
            timestamp_start: 2000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Successful cargo build within 10 minutes
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo build",
            command_binary: Some("cargo"),
            command_subcommand: Some("build"),
            exit_code: Some(0),
            timestamp_start: 2300, // 5 min later
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Query for the resolution
    let mut stmt = conn
        .prepare(
            "SELECT command_raw FROM commands
             WHERE session_id = ?1
               AND command_binary = ?2
               AND exit_code = 0
               AND timestamp_start > ?3
               AND timestamp_start <= ?4
             ORDER BY timestamp_start ASC LIMIT 1",
        )
        .unwrap();

    let fix: String = stmt
        .query_row(
            rusqlite::params![session, "cargo", 2000i64, 2600i64],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(fix, "cargo build");
}

#[test]
fn resolve_no_resolution_outside_time_window() {
    let conn = setup();
    let session = "sess-resolve-3";

    // Failed cargo test
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo test",
            command_binary: Some("cargo"),
            exit_code: Some(1),
            stderr: Some("error: test failed"),
            timestamp_start: 3000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Successful cargo test but >10 minutes later
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo test",
            command_binary: Some("cargo"),
            exit_code: Some(0),
            timestamp_start: 3700, // 11+ minutes later
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT command_raw FROM commands
             WHERE session_id = ?1
               AND command_binary = ?2
               AND exit_code = 0
               AND timestamp_start > ?3
               AND timestamp_start <= ?4
             ORDER BY timestamp_start ASC LIMIT 1",
        )
        .unwrap();

    let result: Result<String, _> = stmt.query_row(
        rusqlite::params![session, "cargo", 3000i64, 3600i64],
        |row| row.get(0),
    );

    assert!(
        result.is_err(),
        "Should not find resolution outside 10-minute window"
    );
}

// ── Normalization tests ────────────────────────────────────────────────────

#[test]
fn normalize_error_for_matching() {
    let a = "Error: Cannot find module '/Users/foo/node_modules/bcrypt'";
    let b = "Error: Cannot find module '/home/bar/project/node_modules/bcrypt'";

    let norm_a = normalize_error(a);
    let norm_b = normalize_error(b);

    assert_eq!(
        norm_a, norm_b,
        "Same error with different paths should normalize to the same string: {norm_a} vs {norm_b}"
    );
}

#[test]
fn resolve_fallback_to_failed_commands() {
    let conn = setup();
    let session = "sess-resolve-fallback";

    // Insert a failed command without strong FTS-matchable content in command_raw
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "make",
            command_binary: Some("make"),
            exit_code: Some(2),
            stderr: Some("fatal: recipe for target 'build' failed"),
            timestamp_start: 5000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Verify the failed command is retrievable via get_commands with failed_only
    let failed = db::get_commands(
        &conn,
        &CommandFilter {
            failed_only: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        !failed.is_empty(),
        "Should find failed commands via get_commands"
    );
    assert!(
        failed
            .iter()
            .any(|c| c.stderr.as_deref().unwrap_or("").contains("recipe for target")),
        "Should find the make failure"
    );
}
