//! The set of open stores: one index store and one store per Dig, under a
//! data directory that differs between dev and prod builds (§59).

use crate::{Store, StoreKind};
use parking_lot::RwLock;
use riddle_core::DigId;
use riddle_errors::{Result, RiddleError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Stores {
    root: Option<PathBuf>,
    index: Arc<Store>,
    digs: RwLock<HashMap<DigId, Arc<Store>>>,
}

impl Stores {
    /// `root` is the data directory; the index store is `root/index.sqlite`
    /// and each Dig is `root/digs/<dig id>/dig.sqlite`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let index = Store::open(root.join("index.sqlite"), StoreKind::Index)?;
        Ok(Self { root: Some(root), index: Arc::new(index), digs: RwLock::new(HashMap::new()) })
    }

    /// Everything in memory: the test-host guard uses this so a test can never
    /// open the real store.
    pub fn in_memory() -> Result<Self> {
        Ok(Self { root: None, index: Arc::new(Store::in_memory(StoreKind::Index)?), digs: RwLock::new(HashMap::new()) })
    }

    pub fn root(&self) -> Option<&Path> { self.root.as_deref() }
    pub fn index(&self) -> Arc<Store> { self.index.clone() }

    pub fn dig_dir(&self, dig: &DigId) -> Option<PathBuf> {
        self.root.as_ref().map(|r| r.join("digs").join(dig.as_str()))
    }

    /// Open (lazily) the store for a Dig.
    pub fn dig(&self, dig: &DigId) -> Result<Arc<Store>> {
        if let Some(s) = self.digs.read().get(dig) {
            return Ok(s.clone());
        }
        let store = match &self.root {
            Some(root) => Store::open(root.join("digs").join(dig.as_str()).join("dig.sqlite"), StoreKind::Dig)?,
            None => Store::in_memory(StoreKind::Dig)?,
        };
        let store = Arc::new(store);
        self.digs.write().insert(dig.clone(), store.clone());
        Ok(store)
    }

    pub fn open_digs(&self) -> Vec<(DigId, Arc<Store>)> {
        self.digs.read().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Remove a Dig's store from disk (§13). The caller purges keychain items.
    pub fn remove_dig(&self, dig: &DigId) -> Result<()> {
        self.digs.write().remove(dig);
        if let Some(dir) = self.dig_dir(dig) {
            if dir.exists() {
                std::fs::remove_dir_all(&dir).map_err(|e| RiddleError::store(format!("remove {}: {e}", dir.display())))?;
            }
        }
        Ok(())
    }

    /// Any faulted store, for the Task Centre.
    pub fn any_fault(&self) -> Option<String> {
        if let Some(f) = self.index.fault() { return Some(f); }
        self.digs.read().values().find_map(|s| s.fault())
    }

    pub fn total_size_bytes(&self) -> u64 {
        self.index.size_bytes() + self.digs.read().values().map(|s| s.size_bytes()).sum::<u64>()
    }
}
