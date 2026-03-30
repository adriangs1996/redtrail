use redtrail::core::db::{self, CommandFilter, NewCommand};
use redtrail::core::errors::{detect_error_fix_sequences, normalize_error};

// ── normalize_error tests ────────────────────────────────────────────────────

#[test]
fn normalize_strips_ansi_codes() {
    let input = "\x1b[31merror: something failed\x1b[0m";
    let result = normalize_error(input);
    assert!(
        !result.contains("\x1b"),
        "expected ANSI codes stripped, got: {result}"
    );
    assert!(result.contains("error:"), "got: {result}");
}

#[test]
fn normalize_strips_file_paths() {
    let input = "error: file not found at /Users/adrian/projects/foo/src/bar.rs";
    let result = normalize_error(input);
    assert!(
        !result.contains("/Users"),
        "expected path stripped, got: {result}"
    );
    assert!(result.contains("<path>"), "got: {result}");
}

#[test]
fn normalize_strips_line_numbers() {
    let input = "error: unexpected token at :42:13:";
    let result = normalize_error(input);
    assert!(
        !result.contains(":42:13:"),
        "expected line numbers stripped, got: {result}"
    );
    assert!(result.contains(":<line>:"), "got: {result}");
}

#[test]
fn normalize_lowercases() {
    let input = "ERROR: Module Not Found";
    let result = normalize_error(input);
    assert_eq!(result, "error: module not found");
}

#[test]
fn normalize_trims_whitespace() {
    let input = "   error: something   ";
    let result = normalize_error(input);
    assert_eq!(result, "error: something");
}

#[test]
fn normalize_strips_timestamps() {
    let input = "2024-01-15T12:34:56Z error: connection refused";
    let result = normalize_error(input);
    assert!(
        !result.contains("2024-01-15"),
        "expected timestamp stripped, got: {result}"
    );
    assert!(result.contains("error:"), "got: {result}");
}

// ── detect_error_fix_sequences tests ────────────────────────────────────────

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn detect_simple_error_fix_sequence() {
    let conn = setup();
    let session = "sess-abc";

    // cargo test fails
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo test",
            command_binary: Some("cargo"),
            command_subcommand: Some("test"),
            exit_code: Some(1),
            stderr: Some("error[E0308]: mismatched types"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // fix action: edit a file
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "vim src/main.rs",
            command_binary: Some("vim"),
            exit_code: Some(0),
            timestamp_start: 1010,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // cargo test succeeds
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo test",
            command_binary: Some("cargo"),
            command_subcommand: Some("test"),
            exit_code: Some(0),
            timestamp_start: 1020,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let commands = db::get_commands(&conn, &CommandFilter::default()).unwrap();
    let sequences = detect_error_fix_sequences(&commands);

    assert_eq!(sequences.len(), 1, "expected 1 sequence");
    let seq = &sequences[0];
    assert!(seq.resolved, "expected resolved");
    assert_eq!(seq.fix_actions.len(), 1, "expected 1 fix action");
    assert_eq!(seq.fix_actions[0], "vim src/main.rs");
    assert_eq!(
        seq.resolution_command.as_deref(),
        Some("cargo test"),
        "resolution should be the successful cargo test"
    );
}

#[test]
fn detect_unresolved_error() {
    let conn = setup();
    let session = "sess-unresolved";

    // npm test fails with no follow-up recovery
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "npm test",
            command_binary: Some("npm"),
            command_subcommand: Some("test"),
            exit_code: Some(1),
            stderr: Some("FAIL src/app.test.js"),
            timestamp_start: 2000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let commands = db::get_commands(&conn, &CommandFilter::default()).unwrap();
    let sequences = detect_error_fix_sequences(&commands);

    assert_eq!(sequences.len(), 1, "expected 1 sequence");
    let seq = &sequences[0];
    assert!(!seq.resolved, "expected unresolved");
    assert!(seq.resolution_command.is_none());
}

#[test]
fn ignores_read_only_commands_in_fix_actions() {
    let conn = setup();
    let session = "sess-readonly";

    // cargo build fails
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo build",
            command_binary: Some("cargo"),
            command_subcommand: Some("build"),
            exit_code: Some(1),
            stderr: Some("error: cannot find value `foo` in this scope"),
            timestamp_start: 3000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // read-only: cat a file (should be excluded from fix_actions)
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cat src/lib.rs",
            command_binary: Some("cat"),
            exit_code: Some(0),
            timestamp_start: 3010,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // write action: edit the file (should be included)
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "vim src/lib.rs",
            command_binary: Some("vim"),
            exit_code: Some(0),
            timestamp_start: 3020,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // cargo build succeeds
    db::insert_command(
        &conn,
        &NewCommand {
            session_id: session,
            command_raw: "cargo build",
            command_binary: Some("cargo"),
            command_subcommand: Some("build"),
            exit_code: Some(0),
            timestamp_start: 3030,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let commands = db::get_commands(&conn, &CommandFilter::default()).unwrap();
    let sequences = detect_error_fix_sequences(&commands);

    assert_eq!(sequences.len(), 1, "expected 1 sequence");
    let seq = &sequences[0];
    assert!(seq.resolved, "expected resolved");
    assert_eq!(
        seq.fix_actions.len(),
        1,
        "only the vim edit should be in fix_actions, not cat"
    );
    assert_eq!(seq.fix_actions[0], "vim src/lib.rs");
}
