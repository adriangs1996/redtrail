use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn create_session_stores_and_retrieves() {
    let conn = setup();

    let id = db::create_session(
        &conn,
        &db::NewSession {
            cwd_initial: Some("/home/user/project"),
            hostname: Some("devbox"),
            shell: Some("zsh"),
            source: "human",
        },
    )
    .unwrap();

    let session = db::get_session(&conn, &id).unwrap();
    assert_eq!(session.id, id);
    assert_eq!(session.cwd_initial.as_deref(), Some("/home/user/project"));
    assert_eq!(session.shell.as_deref(), Some("zsh"));
    assert_eq!(session.source, "human");
    assert!(session.started_at > 0);
}

#[test]
fn create_session_generates_unique_ids() {
    let conn = setup();
    let s = db::NewSession { source: "human", ..Default::default() };

    let id1 = db::create_session(&conn, &s).unwrap();
    let id2 = db::create_session(&conn, &s).unwrap();
    assert_ne!(id1, id2);
}

#[test]
fn commands_linked_to_session() {
    let conn = setup();
    let sid = db::create_session(
        &conn,
        &db::NewSession { source: "human", ..Default::default() },
    )
    .unwrap();

    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "ls",
        timestamp_start: 1000,
        source: "human",
        ..Default::default()
    }).unwrap();

    db::insert_command(&conn, &db::NewCommand {
        session_id: &sid,
        command_raw: "pwd",
        timestamp_start: 1001,
        source: "human",
        ..Default::default()
    }).unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter {
        session_id: Some(&sid),
        ..Default::default()
    }).unwrap();

    assert_eq!(cmds.len(), 2);
    assert!(cmds.iter().all(|c| c.session_id == sid));
}

#[test]
fn list_sessions_returns_recent() {
    let conn = setup();

    let s1 = db::create_session(&conn, &db::NewSession {
        cwd_initial: Some("/project-a"),
        source: "human",
        ..Default::default()
    }).unwrap();

    let s2 = db::create_session(&conn, &db::NewSession {
        cwd_initial: Some("/project-b"),
        source: "human",
        ..Default::default()
    }).unwrap();

    let sessions = db::list_sessions(&conn, 10).unwrap();
    assert_eq!(sessions.len(), 2);
    // Both sessions present (order may vary if same timestamp)
    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&s1.as_str()));
    assert!(ids.contains(&s2.as_str()));
}

#[test]
fn get_session_nonexistent_returns_error() {
    let conn = setup();
    let result = db::get_session(&conn, "nonexistent");
    assert!(result.is_err());
}
