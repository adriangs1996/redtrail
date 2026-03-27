use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn seed(conn: &rusqlite::Connection) {
    db::insert_command(conn, &db::NewCommand {
        session_id: "s1",
        command_raw: "cargo build --release",
        command_binary: Some("cargo"),
        stdout: Some("Compiling redtrail v0.2.0\nFinished release target"),
        timestamp_start: 1000,
        source: "human",
        ..Default::default()
    }).unwrap();

    db::insert_command(conn, &db::NewCommand {
        session_id: "s1",
        command_raw: "git push origin main",
        command_binary: Some("git"),
        stdout: Some("To github.com:user/repo.git\n  abc123..def456 main -> main"),
        timestamp_start: 1001,
        source: "human",
        ..Default::default()
    }).unwrap();

    db::insert_command(conn, &db::NewCommand {
        session_id: "s1",
        command_raw: "docker build -t myapp .",
        command_binary: Some("docker"),
        stderr: Some("Error: failed to solve: dockerfile parse error"),
        exit_code: Some(1),
        timestamp_start: 1002,
        source: "human",
        ..Default::default()
    }).unwrap();
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
fn search_respects_limit() {
    let conn = setup();
    // Insert many matching commands
    for i in 0..10 {
        db::insert_command(&conn, &db::NewCommand {
            session_id: "s1",
            command_raw: &format!("echo test-{i}"),
            command_binary: Some("echo"),
            timestamp_start: 1000 + i,
            source: "human",
            ..Default::default()
        }).unwrap();
    }

    let results = db::search_commands(&conn, "test", 3).unwrap();
    assert_eq!(results.len(), 3);
}
