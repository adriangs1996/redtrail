use redtrail::core::db;
use redtrail::extract;

#[test]
fn inline_extraction_creates_entities_for_git_commands() {
    let conn = db::open_in_memory().unwrap();

    // Create session
    conn.execute(
        "INSERT INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    )
    .unwrap();

    // Simulate capture start
    let id = db::insert_command_start(
        &conn,
        &db::NewCommandStart {
            session_id: "sess",
            command_raw: "git status",
            command_binary: Some("git"),
            command_subcommand: Some("status"),
            command_args: None,
            command_flags: None,
            cwd: Some("/home/user/project"),
            shell: Some("zsh"),
            hostname: None,
            source: "human",
            redacted: false,
        },
    )
    .unwrap();

    // Simulate tee writing output
    db::update_command_output(&conn, &id, Some(" M src/main.rs\n?? new.txt\n"), None, false, false)
        .unwrap();

    // Simulate capture finish
    db::finish_command(
        &conn,
        &db::FinishCommand {
            command_id: &id,
            exit_code: Some(0),
            git_repo: Some("/home/user/project"),
            git_branch: Some("main"),
            env_snapshot: None,
            stdout: None,
            stderr: None,
        },
    )
    .unwrap();

    // Run inline extraction (same as try_inline_extraction would do)
    let cmd = extract::db::get_command_by_id(&conn, &id).unwrap();
    let _ = extract::extract_command(&conn, &cmd, None);

    // Verify entities were created
    let entities = extract::db::get_entities(
        &conn,
        &extract::db::EntityFilter {
            entity_type: Some("git_file"),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        entities.iter().any(|e| e.name == "src/main.rs"),
        "should have extracted git_file entity for src/main.rs"
    );
    assert!(
        entities.iter().any(|e| e.name == "new.txt"),
        "should have extracted git_file entity for new.txt"
    );
}

#[test]
fn inline_extraction_skips_generic_domain() {
    let conn = db::open_in_memory().unwrap();

    conn.execute(
        "INSERT INTO sessions (id, started_at, source) VALUES ('sess', 1000, 'human')",
        [],
    )
    .unwrap();

    // Insert a non-git command
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary, stdout, source, status)
         VALUES ('cmd-1', 'sess', 1000, 'ls -la', 'ls', 'total 42\ndrwxr-xr-x file.txt\n', 'human', 'finished')",
        [],
    )
    .unwrap();

    // Simulate the domain check that try_inline_extraction does
    let cmd = extract::db::get_command_by_id(&conn, "cmd-1").unwrap();
    let binary = cmd.command_binary.as_deref().unwrap_or("");
    let domain = redtrail::extract::domain::detect_domain(binary);

    // Generic domain should NOT trigger inline extraction
    assert_eq!(domain, redtrail::extract::types::Domain::Generic);

    // Command should remain unextracted
    let unextracted = extract::db::get_unextracted_commands(&conn, None, 100).unwrap();
    assert_eq!(unextracted.len(), 1);
}
