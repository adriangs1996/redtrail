use redtrail::db_v2::DbV2;
use redtrail::workflows::query::{QueryWorkflow, QueryInput};
use redtrail::workflows::session::{SessionContext, SessionWorkflow};

#[test]
fn rejects_non_select_statements() {
    let db = DbV2::open_in_memory().unwrap();
    let wf = QueryWorkflow::new();
    let input = QueryInput { raw: "DROP TABLE hosts".into() };
    let result = wf.run(&db, input);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("read-only"));
}

#[test]
fn accepts_select_query() {
    let db = DbV2::open_in_memory().unwrap();
    let ctx = SessionContext::new("test".into());
    SessionWorkflow::save(&db, &ctx).unwrap();

    let wf = QueryWorkflow::new();
    let input = QueryInput { raw: "SELECT name, status FROM sessions".into() };
    let result = wf.run(&db, input).unwrap();
    assert!(result.formatted.contains("test"));
}

#[test]
fn accepts_pragma_query() {
    let db = DbV2::open_in_memory().unwrap();
    let wf = QueryWorkflow::new();
    let input = QueryInput { raw: "PRAGMA table_info(sessions)".into() };
    let result = wf.run(&db, input);
    assert!(result.is_ok());
}

#[test]
fn accepts_explain_query() {
    let db = DbV2::open_in_memory().unwrap();
    let wf = QueryWorkflow::new();
    let input = QueryInput { raw: "EXPLAIN SELECT * FROM sessions".into() };
    let result = wf.run(&db, input);
    assert!(result.is_ok());
}
