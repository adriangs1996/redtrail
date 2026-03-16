use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use rusqlite::params;
use crate::db_v2::DbV2;
use crate::middleware::events::{EventListener, ShellEvent};
use crate::workflows::session::SessionContext;

struct PendingCommand {
    cmd: String,
    started_at: std::time::Instant,
}

pub struct LoggerListener {
    db: Arc<Mutex<DbV2>>,
    pending: Mutex<HashMap<u32, PendingCommand>>,
}

impl LoggerListener {
    pub fn new(db: Arc<Mutex<DbV2>>) -> Self {
        Self {
            db,
            pending: Mutex::new(HashMap::new()),
        }
    }
}

impl EventListener for LoggerListener {
    fn on_event(&self, event: &ShellEvent, ctx: &SessionContext) {
        match event {
            ShellEvent::CommandStarted { cmd, job_id } => {
                self.pending.lock().unwrap().insert(*job_id, PendingCommand {
                    cmd: cmd.clone(),
                    started_at: std::time::Instant::now(),
                });
            }
            ShellEvent::CommandFinished { job_id, exit_code, output } => {
                if let Some(pending) = self.pending.lock().unwrap().remove(job_id) {
                    let duration_ms = pending.started_at.elapsed().as_millis() as i64;
                    let db = self.db.lock().unwrap();
                    let _ = db.conn().execute(
                        "INSERT INTO command_history (session_id, command, exit_code, duration_ms, output_preview)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![ctx.id, pending.cmd, exit_code, duration_ms, output],
                    );
                }
            }
            _ => {}
        }
    }
}
