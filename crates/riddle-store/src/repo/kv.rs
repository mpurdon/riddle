//! Small key/value state in the index store: cursors, flags, update state.

use crate::{now_ts, Store};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn get(store: &Store, key: &str) -> Result<Option<String>> {
    store.read(|c| {
        c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()
    })
}

pub fn set(store: &Store, key: &str, value: &str) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO kv (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now_ts()],
        )?;
        Ok(())
    })
}

pub fn get_json<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> Result<Option<T>> {
    Ok(get(store, key)?.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn set_json<T: serde::Serialize>(store: &Store, key: &str, value: &T) -> Result<()> {
    set(store, key, &serde_json::to_string(value)?)
}

/// Every key under a prefix with its value, for per-message marks such as
/// Shadow reviews (`shadow_review:<dig id>:<message id>`). A range on the
/// key uses the primary-key index; `substr()` or LIKE would scan the table.
pub fn list_prefix(store: &Store, prefix: &str) -> Result<Vec<(String, String)>> {
    store.read(|c| {
        let mut st =
            c.prepare("SELECT key, value FROM kv WHERE key >= ?1 AND key < ?2 ORDER BY key")?;
        let rows = st
            .query_map(
                rusqlite::params![prefix, format!("{prefix}\u{10FFFF}")],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?
            .collect();
        rows
    })
}
