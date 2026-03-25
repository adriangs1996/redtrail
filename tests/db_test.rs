use redtrail::core::db;
use redtrail::core::extractor::{Fact, Relation};

fn setup() -> rusqlite::Connection {
    db::open_in_memory().unwrap()
}

fn setup_with_session(workspace: &str) -> (rusqlite::Connection, String) {
    let conn = setup();
    let sid = db::ensure_session(&conn, workspace).unwrap();
    (conn, sid)
}

#[test]
fn open_in_memory_creates_schema() {
    let conn = setup();
    for table in &["sessions", "events", "facts", "relations"] {
        let exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert!(exists, "table '{table}' should exist");
    }
}

#[test]
fn ensure_session_creates_new() {
    let conn = setup();
    let sid = db::ensure_session(&conn, "/home/user/myproject").unwrap();
    assert!(sid.starts_with("myproject-"), "got: {sid}");
}

#[test]
fn ensure_session_reuses_existing() {
    let conn = setup();
    let sid1 = db::ensure_session(&conn, "/home/user/myproject").unwrap();
    let sid2 = db::ensure_session(&conn, "/home/user/myproject").unwrap();
    assert_eq!(sid1, sid2);
}

#[test]
fn insert_event_and_query_back() {
    let (conn, sid) = setup_with_session("/tmp/test");

    let eid = db::insert_event(
        &conn, &sid, "nmap -sV 10.10.10.1", Some("nmap"),
        0, 1500, "22/tcp open ssh", "abc123",
    ).unwrap();
    assert!(eid > 0);

    let (cmd, tool, status): (String, Option<String>, String) = conn
        .query_row(
            "SELECT command, tool, extraction_status FROM events WHERE id = ?1",
            [eid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(cmd, "nmap -sV 10.10.10.1");
    assert_eq!(tool.as_deref(), Some("nmap"));
    assert_eq!(status, "stored");
}

#[test]
fn fact_upsert_merges_attributes() {
    let (conn, sid) = setup_with_session("/tmp/test");
    let eid = db::insert_event(&conn, &sid, "nmap", None, 0, 0, "", "").unwrap();

    // First insert: regex, confidence 1.0
    let attrs1 = serde_json::json!({"ip": "10.10.10.1", "status": "up"});
    db::insert_fact(&conn, &sid, eid, "host", "host:10.10.10.1", &attrs1, 1.0, "regex").unwrap();

    // Upsert: llm, confidence 0.8
    let attrs2 = serde_json::json!({"hostname": "target.htb"});
    db::insert_fact(&conn, &sid, eid, "host", "host:10.10.10.1", &attrs2, 0.8, "llm").unwrap();

    let (attr_str, conf, source): (String, f64, String) = conn
        .query_row(
            "SELECT attributes, confidence, source FROM facts WHERE key = 'host:10.10.10.1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&attr_str).unwrap();
    // json_patch merges both
    assert_eq!(parsed["ip"], "10.10.10.1");
    assert_eq!(parsed["hostname"], "target.htb");
    // MAX(1.0, 0.8) = 1.0
    assert!((conf - 1.0).abs() < 0.001);
    // source stays "regex" since 1.0 > 0.8
    assert_eq!(source, "regex");
}

#[test]
fn insert_relation_and_dedup() {
    let (conn, sid) = setup_with_session("/tmp/test");

    db::insert_relation(&conn, &sid, "service:10.10.10.1:22/tcp", "host:10.10.10.1", "runs_on").unwrap();
    db::insert_relation(&conn, &sid, "service:10.10.10.1:22/tcp", "host:10.10.10.1", "runs_on").unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM relations WHERE session_id = ?1", [&sid], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn store_extraction_transaction() {
    let (conn, sid) = setup_with_session("/tmp/test");
    let eid = db::insert_event(&conn, &sid, "nmap -sV 10.10.10.1", Some("nmap"), 0, 1000, "output", "hash").unwrap();

    let facts = vec![
        Fact { fact_type: "host".into(), key: "host:10.10.10.1".into(), attributes: serde_json::json!({"ip": "10.10.10.1"}) },
        Fact { fact_type: "service".into(), key: "service:10.10.10.1:22/tcp".into(), attributes: serde_json::json!({"port": 22, "service": "ssh"}) },
    ];
    let relations = vec![
        Relation { from_key: "service:10.10.10.1:22/tcp".into(), to_key: "host:10.10.10.1".into(), relation_type: "runs_on".into() },
    ];

    db::store_extraction(&conn, &sid, eid, &facts, &relations).unwrap();

    let fact_count: i64 = conn.query_row("SELECT COUNT(*) FROM facts", [], |r| r.get(0)).unwrap();
    assert_eq!(fact_count, 2);

    let rel_count: i64 = conn.query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0)).unwrap();
    assert_eq!(rel_count, 1);

    let status: String = conn.query_row("SELECT extraction_status FROM events WHERE id = ?1", [eid], |r| r.get(0)).unwrap();
    assert_eq!(status, "extracted");
}
