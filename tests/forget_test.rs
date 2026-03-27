use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn seed(conn: &rusqlite::Connection) -> (String, String, String) {
    let sid = db::create_session(conn, &db::NewSession {
        source: "human", ..Default::default()
    }).unwrap();

    let id1 = db::insert_command(conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo old",
        timestamp_start: 1000,
        source: "human",
        ..Default::default()
    }).unwrap();

    let id2 = db::insert_command(conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "echo recent",
        timestamp_start: 9999999999, // far future
        source: "human",
        ..Default::default()
    }).unwrap();

    (sid, id1, id2)
}

#[test]
fn forget_command_deletes_single_row() {
    let conn = setup();
    let (_sid, id1, _id2) = seed(&conn);

    db::forget_command(&conn, &id1).unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_ne!(cmds[0].id, id1, "deleted command should be gone");
}

#[test]
fn forget_session_deletes_all_commands_in_session() {
    let conn = setup();
    let (sid, _id1, _id2) = seed(&conn);

    db::forget_session(&conn, &sid).unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert!(cmds.is_empty(), "all commands in session should be deleted");

    // Session row itself should also be gone
    assert!(db::get_session(&conn, &sid).is_err());
}

#[test]
fn forget_since_deletes_recent_commands() {
    let conn = setup();
    let (_sid, _id1, _id2) = seed(&conn);

    // Delete everything after timestamp 5000
    db::forget_since(&conn, 5000).unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "echo old");
}

#[test]
fn forget_command_also_removes_from_fts() {
    let conn = setup();
    let (_sid, id1, _id2) = seed(&conn);

    db::forget_command(&conn, &id1).unwrap();

    let results = db::search_commands(&conn, "old", 50).unwrap();
    assert!(results.is_empty(), "FTS should not find deleted command");
}
