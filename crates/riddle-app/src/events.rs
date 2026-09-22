//! A broadcast EventSink: the desktop bridges it to Tauri events, the CLI
//! prints NDJSON from it, tests inspect it. Emitting never fails the work.

use riddle_core::{EventSink, RiddleEvent};
use tokio::sync::broadcast;

pub struct Broadcast {
    tx: broadcast::Sender<RiddleEvent>,
}

impl Default for Broadcast {
    fn default() -> Self { Self::new() }
}

impl Broadcast {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self { tx }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<RiddleEvent> { self.tx.subscribe() }
}

impl EventSink for Broadcast {
    fn emit(&self, event: RiddleEvent) {
        // No subscribers is not an error; a lagging subscriber drops old events.
        let _ = self.tx.send(event);
    }
}
