use redtrail::db_v2::DbV2;
use redtrail::workflows::session::{SessionContext, SessionWorkflow};
use rusqlite::params;

#[test]
fn clone_copies_all_related_data() {
    let db = DbV2::open_in_memory().unwrap();
    let src = SessionContext::new("source".into());
    SessionWorkflow::save(&db, &src).unwrap();

    db.conn().execute(
        "INSERT INTO hosts (session_id, ip, hostname) VALUES (?1, ?2, ?3)",
        params![src.id, "10.10.10.1", "target"],
    ).unwrap();
    let host_id: i64 = db.conn().last_insert_rowid();

    db.conn().execute(
        "INSERT INTO credentials (session_id, username, password, host_id) VALUES (?1, ?2, ?3, ?4)",
        params![src.id, "admin", "password123", host_id],
    ).unwrap();

    SessionWorkflow::clone_session(&db, &src.id, "cloned").unwrap();

    let cloned = SessionWorkflow::load_by_name(&db, "cloned").unwrap();
    assert_ne!(cloned.id, src.id);

    let host_count: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM hosts WHERE session_id = ?1",
        params![cloned.id], |row| row.get(0),
    ).unwrap();
    assert_eq!(host_count, 1);

    let cred_count: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM credentials WHERE session_id = ?1",
        params![cloned.id], |row| row.get(0),
    ).unwrap();
    assert_eq!(cred_count, 1);
}
