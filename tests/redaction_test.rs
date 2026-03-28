use redtrail::core::db;

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

#[test]
fn command_with_aws_key_is_redacted_before_storage() {
    let conn = setup();

    db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds.len(), 1);
    assert!(
        cmds[0].command_raw.contains("[REDACTED:"),
        "command should be redacted in DB, got: {}",
        cmds[0].command_raw
    );
    assert!(
        !cmds[0].command_raw.contains("AKIAIOSFODNN7EXAMPLE"),
        "raw key should NOT be in DB"
    );
    assert!(cmds[0].redacted, "redacted flag should be true");
}

#[test]
fn command_with_jwt_in_stdout_is_redacted() {
    let conn = setup();

    db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "curl https://api.example.com",
            stdout: Some("token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    let stdout = cmds[0].stdout.as_deref().unwrap();
    assert!(
        stdout.contains("[REDACTED:jwt]"),
        "stdout should have JWT redacted, got: {stdout}"
    );
    assert!(cmds[0].redacted);
}

#[test]
fn clean_command_not_marked_redacted() {
    let conn = setup();

    db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "ls -la",
            stdout: Some("total 42\ndrwxr-xr-x  5 user  staff  160 Mar 27 10:00 ."),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert_eq!(cmds[0].command_raw, "ls -la");
    assert!(!cmds[0].redacted, "clean command should not be marked redacted");
}

#[test]
fn password_in_command_is_redacted() {
    let conn = setup();

    db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "PASSWORD=hunter2 ./deploy.sh",
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert!(
        !cmds[0].command_raw.contains("hunter2"),
        "password value should not be in DB, got: {}",
        cmds[0].command_raw
    );
    assert!(cmds[0].redacted);
}

#[test]
fn redaction_audit_log_records_events() {
    let conn = setup();

    let cmd_id = db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE",
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let logs = db::get_redaction_logs(&conn, &cmd_id).unwrap();
    assert!(!logs.is_empty(), "should have redaction log entries");
    assert!(
        logs.iter().any(|l| l.field == "command_raw"),
        "should log which field was redacted"
    );
    assert!(
        logs.iter().any(|l| l.pattern_label == "aws_key"),
        "should log the pattern label"
    );
}

#[test]
fn clean_command_has_no_redaction_logs() {
    let conn = setup();

    let cmd_id = db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "ls -la",
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let logs = db::get_redaction_logs(&conn, &cmd_id).unwrap();
    assert!(logs.is_empty(), "clean command should have no redaction logs");
}

#[test]
fn redaction_audit_log_covers_multiple_fields() {
    let conn = setup();

    let cmd_id = db::insert_command_redacted(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "PASSWORD=hunter2 ./deploy.sh",
            stdout: Some("token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"),
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let logs = db::get_redaction_logs(&conn, &cmd_id).unwrap();
    let fields: Vec<&str> = logs.iter().map(|l| l.field.as_str()).collect();
    assert!(fields.contains(&"command_raw"), "should log command_raw redaction");
    assert!(fields.contains(&"stdout"), "should log stdout redaction");
}
