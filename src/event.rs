use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, KeyEventKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Key(crossterm::event::KeyEvent),
    Tick,
}

pub fn read_event(timeout: Duration) -> Option<Event> {
    if crossterm::event::poll(timeout).ok()? {
        match crossterm::event::read().ok()? {
            CrosstermEvent::Key(k) if k.kind == KeyEventKind::Press => Some(Event::Key(k)),
            _ => None,
        }
    } else {
        Some(Event::Tick)
    }
}

pub fn spawn_tick_channel(
    _interval: Duration,
) -> (
    tokio::sync::mpsc::UnboundedSender<Event>,
    tokio::sync::mpsc::UnboundedReceiver<Event>,
) {
    tokio::sync::mpsc::unbounded_channel()
}
