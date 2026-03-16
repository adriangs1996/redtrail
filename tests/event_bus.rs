use redtrail::middleware::events::{EventBus, EventListener, ShellEvent};
use redtrail::workflows::session::SessionContext;
use std::sync::{Arc, Mutex};

struct RecordingListener {
    events: Arc<Mutex<Vec<String>>>,
}

impl EventListener for RecordingListener {
    fn on_event(&self, event: &ShellEvent, _ctx: &SessionContext) {
        let name = match event {
            ShellEvent::CommandStarted { .. } => "started",
            ShellEvent::CommandFinished { .. } => "finished",
            ShellEvent::ShellOutputLine { .. } => "output",
            ShellEvent::SessionSwitched { .. } => "switched",
            ShellEvent::KbUpdated { .. } => "kb_updated",
        };
        self.events.lock().unwrap().push(name.to_string());
    }
}

#[test]
fn event_bus_dispatches_to_all_listeners() {
    let events = Arc::new(Mutex::new(vec![]));
    let listener1 = Arc::new(RecordingListener { events: events.clone() });
    let listener2 = Arc::new(RecordingListener { events: events.clone() });

    let mut bus = EventBus::new();
    bus.add(listener1);
    bus.add(listener2);

    let ctx = SessionContext::new("test".into());
    bus.emit(&ShellEvent::CommandStarted { cmd: "nmap".into(), job_id: 1 }, &ctx);

    let recorded = events.lock().unwrap();
    assert_eq!(recorded.len(), 2);
    assert!(recorded.iter().all(|e| e == "started"));
}
