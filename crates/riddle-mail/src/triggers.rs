//! "Refresh now" wiring. The poller for each Dig waits on a `Notify`; the
//! `refresh_now` Action and Demo Mode's injection poke it. Defined here
//! rather than in `riddle-sync` so Actions in this crate can trigger a
//! refresh without depending on the sync engine.

use parking_lot::RwLock;
use riddle_core::DigId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Default)]
pub struct RefreshTriggers {
    per_dig: RwLock<HashMap<DigId, Arc<Notify>>>,
}

impl RefreshTriggers {
    pub fn new() -> Self {
        Self::default()
    }

    /// The Notify a poller for `dig` selects on; created on first use.
    pub fn for_dig(&self, dig: &DigId) -> Arc<Notify> {
        if let Some(n) = self.per_dig.read().get(dig) {
            return n.clone();
        }
        let n = Arc::new(Notify::new());
        self.per_dig.write().insert(dig.clone(), n.clone());
        n
    }

    /// Wake one poller, or all of them.
    pub fn trigger(&self, dig: Option<&DigId>) -> usize {
        match dig {
            Some(d) => {
                self.for_dig(d).notify_one();
                1
            }
            None => {
                let all = self.per_dig.read();
                for n in all.values() {
                    n.notify_one();
                }
                all.len()
            }
        }
    }

    pub fn remove(&self, dig: &DigId) {
        self.per_dig.write().remove(dig);
    }
}
