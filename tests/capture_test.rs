use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn insert_command_stores_and_retrieves() {
    let conn = setup();

    let id = db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "sess-1",
            command_raw: "echo hello",
            command_binary: Some("echo"),
            cwd: Some("/tmp"),
            exit_code: Some(0),
            hostname: Some("localhost"),
            shell: Some("zsh"),
            source: "human",
            timestamp_start: 1000,
            timestamp_end: Some(1001),
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].id, id);
    assert_eq!(cmds[0].command_raw, "echo hello");
    assert_eq!(cmds[0].command_binary.as_deref(), Some("echo"));
    assert_eq!(cmds[0].exit_code, Some(0));
    assert_eq!(cmds[0].cwd.as_deref(), Some("/tmp"));
    assert_eq!(cmds[0].source, "human");
}

#[test]
fn insert_command_generates_uuid_id() {
    let conn = setup();

    let id1 = db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "sess-1",
            command_raw: "ls",
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let id2 = db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "sess-1",
            command_raw: "pwd",
            timestamp_start: 1001,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    assert_ne!(id1, id2, "each command should get a unique ID");
    assert!(id1.len() >= 32, "ID should be UUID-like, got: {id1}");
}

#[test]
fn get_commands_filters_by_exit_code() {
    let conn = setup();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "true",
            exit_code: Some(0),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "false",
            exit_code: Some(1),
            timestamp_start: 1001,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let failed = db::get_commands(
        &conn,
        &db::CommandFilter {
            failed_only: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].command_raw, "false");
}

#[test]
fn get_commands_filters_by_binary() {
    let conn = setup();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "git status",
            command_binary: Some("git"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "ls -la",
            command_binary: Some("ls"),
            timestamp_start: 1001,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let git_cmds = db::get_commands(
        &conn,
        &db::CommandFilter {
            command_binary: Some("git"),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(git_cmds.len(), 1);
    assert_eq!(git_cmds[0].command_raw, "git status");
}

#[test]
fn get_commands_filters_by_cwd() {
    let conn = setup();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "make",
            cwd: Some("/home/user/project"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "ls",
            cwd: Some("/tmp"),
            timestamp_start: 1001,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let project_cmds = db::get_commands(
        &conn,
        &db::CommandFilter {
            cwd: Some("/home/user/project"),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(project_cmds.len(), 1);
    assert_eq!(project_cmds[0].command_raw, "make");
}

#[test]
fn get_commands_respects_limit() {
    let conn = setup();

    for i in 0..10 {
        db::insert_command(
            &conn,
            &db::NewCommand {
                session_id: "s1",
                command_raw: &format!("cmd-{i}"),
                timestamp_start: 1000 + i,
                source: "human",
                ..Default::default()
            },
        )
        .unwrap();
    }

    let cmds = db::get_commands(
        &conn,
        &db::CommandFilter {
            limit: Some(3),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(cmds.len(), 3);
    // Most recent first
    assert_eq!(cmds[0].command_raw, "cmd-9");
}

#[test]
fn get_commands_filters_by_since_timestamp() {
    let conn = setup();

    // Old command (yesterday-ish)
    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "echo old",
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    // Recent command
    let recent_ts = now_ts();
    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "echo recent",
            timestamp_start: recent_ts,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(
        &conn,
        &db::CommandFilter {
            since: Some(recent_ts - 10),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "echo recent");
}

#[test]
fn insert_command_stores_stderr_truncated() {
    let conn = setup();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "make",
            stderr: Some("error output"),
            stderr_truncated: true,
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert!(cmds[0].stderr_truncated);
}
