//! Pending (unconfirmed) actions and idempotency keys (§51, §52).

use crate::{from_json, now_ts, to_json, Store};
use riddle_core::PendingAction;
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn put_pending(store: &Store, p: &PendingAction) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO pending_actions (id, json, created_at) VALUES (?1, ?2, ?3)",
            params![p.id, to_json(p)?, now_ts()],
        )?;
        Ok(())
    })
}

pub fn get_pending(store: &Store, id: &str) -> Result<Option<PendingAction>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM pending_actions WHERE id = ?1",
            [id],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

pub fn list_pending(store: &Store) -> Result<Vec<PendingAction>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM pending_actions ORDER BY created_at DESC")?;
        let rows = st
            .query_map([], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn remove_pending(store: &Store, id: &str) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM pending_actions WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// Pending actions older than the TTL are dropped; a stale confirmation must
/// not fire hours later.
pub fn expire_pending(store: &Store, ttl_ms: i64) -> Result<u64> {
    store.write(|c| {
        Ok(c.execute(
            "DELETE FROM pending_actions WHERE created_at < ?1",
            [now_ts() - ttl_ms],
        )? as u64)
    })
}

pub fn get_idempotent(store: &Store, key: &str) -> Result<Option<serde_json::Value>> {
    store.read(|c| {
        c.query_row(
            "SELECT result_json FROM idempotency WHERE key = ?1",
            [key],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

pub fn put_idempotent(
    store: &Store,
    key: &str,
    action: &str,
    result: &serde_json::Value,
) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT OR IGNORE INTO idempotency (key, action, result_json, created_at) VALUES (?1, ?2, ?3, ?4)", params![key, action, to_json(result)?, now_ts()])?;
        Ok(())
    })
}
