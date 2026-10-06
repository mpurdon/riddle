//! "Refresh now" wiring: one channel every refresh request goes down, drained
//! by one task in the app that turns each request into a sync job. It used
//! to be a `Notify` per Dig with a listener spawned per Dig at boot, so an
//! account added after launch had no listener and "Sync now" on it woke
//! nobody. Defined here rather than in `riddle-sync` so Actions in this crate
//! can trigger a refresh without depending on the sync engine.

use riddle_core::DigId;
use tokio::sync::broadcast;

pub struct RefreshTriggers {
    tx: broadcast::Sender<Option<DigId>>,
}

impl Default for RefreshTriggers {
    fn default() -> Self {
        Self::new()
    }
}

impl RefreshTriggers {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(64);
        Self { tx }
    }

    /// Ask for one Dig's refresh, or every Dig's with `None`. Returns whether
    /// anything is listening: false in a process with no background work.
    pub fn trigger(&self, dig: Option<&DigId>) -> bool {
        self.tx.send(dig.cloned()).is_ok()
    }

    /// The stream of requests; the app's one drain task holds it.
    pub fn subscribe(&self) -> broadcast::Receiver<Option<DigId>> {
        self.tx.subscribe()
    }
}
