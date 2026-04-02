use redtrail::core::capture;
use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn stdout_under_limit_stored_as_is() {
    let conn = setup();
    let small_output = "hello world";

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "echo hello",
            stdout: Some(small_output),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds[0].stdout.as_deref(), Some("hello world"));
    assert!(!cmds[0].stdout_truncated);
}

#[test]
fn stdout_over_limit_is_truncated() {
    let conn = setup();
    let big_output = "x".repeat(60_000); // 60KB > 50KB default

    let truncated = capture::truncate_output(&big_output, capture::MAX_STDOUT_BYTES);
    assert!(truncated.len() <= capture::MAX_STDOUT_BYTES);

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "cat bigfile",
            stdout: Some(&truncated),
            stdout_truncated: truncated.len() < big_output.len(),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert!(cmds[0].stdout_truncated, "should be marked truncated");
    assert!(cmds[0].stdout.as_ref().unwrap().len() <= capture::MAX_STDOUT_BYTES);
}

#[test]
fn truncate_preserves_content_start() {
    let output = "line1\nline2\nline3\n".to_string() + &"x".repeat(60_000);
    let truncated = capture::truncate_output(&output, capture::MAX_STDOUT_BYTES);
    assert!(
        truncated.starts_with("line1\n"),
        "should preserve beginning of output"
    );
}

#[test]
fn stdout_over_limit_compressed_and_decompressed_roundtrip() {
    let conn = setup();
    let big_output = "line1\nline2\n".to_string() + &"x".repeat(60_000);

    db::insert_command_compressed(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "cat bigfile",
            stdout: Some(&big_output),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
        capture::MAX_STDOUT_BYTES,
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert!(
        cmds[0].stdout_truncated,
        "should be marked truncated/compressed"
    );
    // The full content should be recoverable
    let stdout = cmds[0].stdout.as_ref().unwrap();
    assert_eq!(
        stdout.len(),
        big_output.len(),
        "decompressed stdout should match original length"
    );
    assert!(
        stdout.starts_with("line1\n"),
        "decompressed content should be intact"
    );
}

#[test]
fn stdout_under_limit_not_compressed() {
    let conn = setup();
    let small_output = "hello world";

    db::insert_command_compressed(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "echo hello",
            stdout: Some(small_output),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
        capture::MAX_STDOUT_BYTES,
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds[0].stdout.as_deref(), Some("hello world"));
    assert!(!cmds[0].stdout_truncated);
}

#[test]
fn stderr_over_limit_compressed_and_decompressed_roundtrip() {
    let conn = setup();
    let big_stderr = "error: ".to_string() + &"e".repeat(60_000);

    db::insert_command_compressed(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "failing command",
            stderr: Some(&big_stderr),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
        capture::MAX_STDOUT_BYTES,
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    let stderr = cmds[0].stderr.as_ref().unwrap();
    assert_eq!(
        stderr.len(),
        big_stderr.len(),
        "decompressed stderr should match original length"
    );
    assert!(
        stderr.starts_with("error: "),
        "decompressed stderr should be intact"
    );
}
