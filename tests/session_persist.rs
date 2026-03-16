use redtrail::db_v2::DbV2;
use redtrail::workflows::session::{SessionContext, SessionWorkflow};

#[test]
fn create_and_load_session() {
    let db = DbV2::open_in_memory().unwrap();
    let ctx = SessionContext::new("test-session".into());

    SessionWorkflow::save(&db, &ctx).unwrap();
    let loaded = SessionWorkflow::load(&db, &ctx.id).unwrap();

    assert_eq!(loaded.name, "test-session");
    assert_eq!(loaded.llm_provider, "anthropic-api");
    assert_eq!(loaded.prompt_template, "redtrail:{session} {status}$ ");
}

#[test]
fn list_sessions() {
    let db = DbV2::open_in_memory().unwrap();
    let s1 = SessionContext::new("alpha".into());
    let s2 = SessionContext::new("beta".into());

    SessionWorkflow::save(&db, &s1).unwrap();
    SessionWorkflow::save(&db, &s2).unwrap();

    let list = SessionWorkflow::list(&db).unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn delete_session_cascades() {
    let db = DbV2::open_in_memory().unwrap();
    let ctx = SessionContext::new("to-delete".into());
    SessionWorkflow::save(&db, &ctx).unwrap();

    db.conn().execute(
        "INSERT INTO hosts (session_id, ip) VALUES (?1, ?2)",
        rusqlite::params![ctx.id, "10.10.10.1"],
    ).unwrap();

    SessionWorkflow::delete(&db, &ctx.id).unwrap();

    let count: i64 = db.conn()
        .query_row("SELECT COUNT(*) FROM hosts WHERE session_id = ?1",
            rusqlite::params![ctx.id], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
