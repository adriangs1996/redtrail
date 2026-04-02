use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn seed(conn: &rusqlite::Connection) {
    db::insert_command(
        conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "cargo build --release",
            command_binary: Some("cargo"),
            stdout: Some("Compiling redtrail v0.2.0\nFinished release target"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "git push origin main",
            command_binary: Some("git"),
            stdout: Some("To github.com:user/repo.git\n  abc123..def456 main -> main"),
            timestamp_start: 1001,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "docker build -t myapp .",
            command_binary: Some("docker"),
            stderr: Some("Error: failed to solve: dockerfile parse error"),
            exit_code: Some(1),
            timestamp_start: 1002,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn search_finds_match_in_command_raw() {
    let conn = setup();
    seed(&conn);

    let results = db::search_commands(&conn, "push", 50).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].command_raw, "git push origin main");
}

#[test]
fn search_finds_match_in_stdout() {
    let conn = setup();
    seed(&conn);

    let results = db::search_commands(&conn, "Compiling", 50).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].command_binary.as_deref(), Some("cargo"));
}

#[test]
fn search_finds_match_in_stderr() {
    let conn = setup();
    seed(&conn);

    let results = db::search_commands(&conn, "dockerfile", 50).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].command_binary.as_deref(), Some("docker"));
}

#[test]
fn search_no_match_returns_empty() {
    let conn = setup();
    seed(&conn);

    let results = db::search_commands(&conn, "kubernetes", 50).unwrap();
    assert!(results.is_empty());
}

#[test]
fn search_returns_decompressed_stdout_for_compressed_commands() {
    let conn = setup();
    use redtrail::core::capture;

    let big_stdout = "SEARCHABLE_MARKER ".to_string() + &"z".repeat(60_000);
    db::insert_command_compressed(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "cat bigfile.txt",
            command_binary: Some("cat"),
            stdout: Some(&big_stdout),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
        capture::MAX_STDOUT_BYTES,
    )
    .unwrap();

    let results = db::search_commands(&conn, "SEARCHABLE_MARKER", 50).unwrap();
    assert_eq!(results.len(), 1, "FTS should find the command");
    let stdout = results[0]
        .stdout
        .as_ref()
        .expect("stdout should be decompressed");
    assert!(
        stdout.starts_with("SEARCHABLE_MARKER"),
        "decompressed stdout should start with the marker"
    );
    assert_eq!(
        stdout.len(),
        big_stdout.len(),
        "decompressed stdout should be the full original length"
    );
}

#[test]
fn search_respects_limit() {
    let conn = setup();
    // Insert many matching commands
    for i in 0..10 {
        db::insert_command(
            &conn,
            &db::NewCommand {
                session_id: "s1",
                command_raw: &format!("echo test-{i}"),
                command_binary: Some("echo"),
                timestamp_start: 1000 + i,
                source: "human",
                ..Default::default()
            },
        )
        .unwrap();
    }

    let results = db::search_commands(&conn, "test", 3).unwrap();
    assert_eq!(results.len(), 3);
}
