//! Per-Dig sync cursors and flags.

use crate::{now_ts, Store};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub const HISTORY_CURSOR: &str = "history_cursor";
pub const INITIAL_SYNC_DONE: &str = "initial_sync_done";
pub const LAST_SYNC_AT: &str = "last_sync_at";
pub const OLDEST_SYNCED: &str = "oldest_synced";

pub fn get(store: &Store, key: &str) -> Result<Option<String>> {
    store.read(|c| {
        c.query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()
    })
}

pub fn set(store: &Store, key: &str, value: &str) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT INTO sync_state (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at", params![key, value, now_ts()])?;
        Ok(())
    })
}

pub fn clear(store: &Store, key: &str) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM sync_state WHERE key = ?1", [key])?;
        Ok(())
    })
}
