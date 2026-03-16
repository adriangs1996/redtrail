use redtrail::middleware::events::{EventBus, ShellEvent};
use redtrail::middleware::builtin::logger::LoggerListener;
use redtrail::db_v2::DbV2;
use redtrail::workflows::session::{SessionContext, SessionWorkflow};
use std::sync::{Arc, Mutex};
use rusqlite::params;

#[test]
fn logger_writes_command_to_history() {
    let db = Arc::new(Mutex::new(DbV2::open_in_memory().unwrap()));
    let ctx = SessionContext::new("test".into());
    SessionWorkflow::save(&db.lock().unwrap(), &ctx).unwrap();

    let logger = Arc::new(LoggerListener::new(db.clone()));
    let mut bus = EventBus::new();
    bus.add(logger);

    bus.emit(&ShellEvent::CommandStarted { cmd: "nmap -sV 10.10.10.1".into(), job_id: 1 }, &ctx);
    bus.emit(&ShellEvent::CommandFinished { job_id: 1, exit_code: 0, output: "PORT   STATE SERVICE\n22/tcp open  ssh".into() }, &ctx);

    let count: i64 = db.lock().unwrap().conn().query_row(
        "SELECT COUNT(*) FROM command_history WHERE session_id = ?1",
        params![ctx.id], |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 1);
}
