use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};

pub enum Event {
    /// A keyboard event.
    Key(KeyEvent),
    /// Terminal was resized.
    Resize(u16, u16),
    /// A periodic tick (used to refresh the UI).
    Tick,
}

pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate_ms),
        }
    }

    pub async fn next(&self) -> Event {
        let tick_rate = self.tick_rate;
        tokio::task::spawn_blocking(move || {
            if event::poll(tick_rate).unwrap_or(false) {
                match event::read() {
                    Ok(CrosstermEvent::Key(key)) => return Event::Key(key),
                    Ok(CrosstermEvent::Resize(w, h)) => return Event::Resize(w, h),
                    _ => {}
                }
            }
            Event::Tick
        })
        .await
        .unwrap_or(Event::Tick)
    }
}
