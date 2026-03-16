use redtrail::db_v2::DbV2;
use redtrail::workflows::query::suggest::SqlCompleter;

#[test]
fn suggests_tables_after_from() {
    let db = DbV2::open_in_memory().unwrap();
    let completer = SqlCompleter::from_db(&db).unwrap();
    let suggestions = completer.complete("SELECT * FROM ho");
    assert!(suggestions.iter().any(|s| s == "hosts"));
}

#[test]
fn suggests_columns_after_where() {
    let db = DbV2::open_in_memory().unwrap();
    let completer = SqlCompleter::from_db(&db).unwrap();
    let suggestions = completer.complete("SELECT * FROM hosts WHERE i");
    assert!(suggestions.iter().any(|s| s == "ip" || s == "id"));
}

#[test]
fn suggests_tables_after_join() {
    let db = DbV2::open_in_memory().unwrap();
    let completer = SqlCompleter::from_db(&db).unwrap();
    let suggestions = completer.complete("SELECT * FROM hosts JOIN po");
    assert!(suggestions.iter().any(|s| s == "ports"));
}

#[test]
fn suggests_all_tables_after_from_with_trailing_space() {
    let db = DbV2::open_in_memory().unwrap();
    let completer = SqlCompleter::from_db(&db).unwrap();
    let suggestions = completer.complete("SELECT * FROM ");
    assert!(suggestions.len() >= 10);
}
