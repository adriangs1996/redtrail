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

#[test]
fn enforce_retention_deletes_old_commands() {
    let conn = setup();
    let sid = db::create_session(&conn, &db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    let now = now_secs();

    // Insert a command from 100 days ago
    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo ancient",
        timestamp_start: now - 100 * 86_400,
        source: "human",
        ..Default::default()
    }).unwrap();

    // Insert a recent command
    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo fresh",
        timestamp_start: now - 10 * 86_400,
        source: "human",
        ..Default::default()
    }).unwrap();

    // Retain 90 days — old command should be purged
    let deleted = db::enforce_retention(&conn, 90).unwrap();
    assert_eq!(deleted, 1);

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "echo fresh");
}

#[test]
fn enforce_retention_cleans_fts() {
    let conn = setup();
    let sid = db::create_session(&conn, &db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    let now = now_secs();

    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo ancient",
        timestamp_start: now - 100 * 86_400,
        source: "human",
        ..Default::default()
    }).unwrap();

    db::enforce_retention(&conn, 90).unwrap();

    let results = db::search_commands(&conn, "ancient", 50).unwrap();
    assert!(results.is_empty(), "FTS entries for expired commands should be removed");
}

#[test]
fn enforce_retention_removes_orphaned_sessions() {
    let conn = setup();
    let sid = db::create_session(&conn, &db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    let now = now_secs();

    // Insert only old commands in this session
    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo old",
        timestamp_start: now - 200 * 86_400,
        source: "human",
        ..Default::default()
    }).unwrap();

    db::enforce_retention(&conn, 90).unwrap();

    // Session should be gone since all its commands were deleted
    assert!(db::get_session(&conn, &sid).is_err());
}

#[test]
fn enforce_retention_zero_days_is_noop() {
    let conn = setup();
    let sid = db::create_session(&conn, &db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo keep",
        timestamp_start: 1000,
        source: "human",
        ..Default::default()
    }).unwrap();

    let deleted = db::enforce_retention(&conn, 0).unwrap();
    assert_eq!(deleted, 0);

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
}

#[test]
fn enforce_retention_noop_when_nothing_expired() {
    let conn = setup();
    let sid = db::create_session(&conn, &db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    let now = now_secs();

    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo recent",
        timestamp_start: now - 5 * 86_400,
        source: "human",
        ..Default::default()
    }).unwrap();

    let deleted = db::enforce_retention(&conn, 90).unwrap();
    assert_eq!(deleted, 0);

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
}
