use std::sync::Arc;
use crate::workflows::session::SessionContext;
use crate::workflows::ShellOutputStream;

#[derive(Debug, Clone)]
pub enum ShellEvent {
    CommandStarted { cmd: String, job_id: u32 },
    ShellOutputLine { job_id: u32, line: String, stream: ShellOutputStream },
    CommandFinished { job_id: u32, exit_code: i32, output: String },
    SessionSwitched { from: String, to: String },
    KbUpdated { entry_type: String },
}

pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &ShellEvent, ctx: &SessionContext);
}

pub struct EventBus {
    listeners: Vec<Arc<dyn EventListener>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self { listeners: vec![] }
    }

    pub fn add(&mut self, listener: Arc<dyn EventListener>) {
        self.listeners.push(listener);
    }

    pub fn emit(&self, event: &ShellEvent, ctx: &SessionContext) {
        for listener in &self.listeners {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_event(event, ctx);
            }));
        }
    }
}
