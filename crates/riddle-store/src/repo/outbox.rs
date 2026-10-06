//! The projection outbox: queued provider writes, idempotent, ordered per thread (§15).

use crate::{from_json, now_ts, to_json, Store};
use riddle_errors::Result;
use rusqlite::params;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutboxRow<T> {
    pub id: i64,
    pub thread_id: Option<String>,
    pub message_id: Option<String>,
    pub op: String,
    pub payload: T,
    pub attempts: u32,
    pub idempotency_key: Option<String>,
}

pub fn enqueue<T: Serialize>(
    store: &Store,
    op: &str,
    thread_id: Option<&str>,
    message_id: Option<&str>,
    payload: &T,
    idempotency_key: Option<&str>,
) -> Result<i64> {
    store.write(|c| {
        c.execute(
            "INSERT OR IGNORE INTO outbox (thread_id, message_id, op, json, idempotency_key, next_attempt_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![thread_id, message_id, op, to_json(payload)?, idempotency_key, now_ts()],
        )?;
        Ok(c.last_insert_rowid())
    })
}

/// Due rows, oldest first; a thread with an earlier pending row blocks its later rows.
pub fn due<T: DeserializeOwned>(store: &Store, limit: u32) -> Result<Vec<OutboxRow<T>>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT id, thread_id, message_id, op, json, attempts, idempotency_key FROM outbox o
             WHERE applied_at IS NULL AND next_attempt_at <= ?1
               AND NOT EXISTS (SELECT 1 FROM outbox p WHERE p.applied_at IS NULL AND p.thread_id = o.thread_id AND p.id < o.id AND p.next_attempt_at > ?1)
             ORDER BY id LIMIT ?2",
        )?;
        let rows = st.query_map(params![now_ts(), limit], |r| {
            Ok(OutboxRow {
                id: r.get(0)?,
                thread_id: r.get(1)?,
                message_id: r.get(2)?,
                op: r.get(3)?,
                payload: from_json(r.get(4)?)?,
                attempts: r.get::<_, i64>(5)? as u32,
                idempotency_key: r.get(6)?,
            })
        })?.collect();
        rows
    })
}

pub fn mark_applied(store: &Store, id: i64) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE outbox SET applied_at = ?2, error = NULL WHERE id = ?1",
            params![id, now_ts()],
        )?;
        Ok(())
    })
}

/// Record a failure and schedule the retry with exponential backoff, capped at an hour.
pub fn mark_failed(store: &Store, id: i64, error: &str) -> Result<()> {
    store.write(|c| {
        let attempts: i64 = c.query_row("SELECT attempts FROM outbox WHERE id = ?1", [id], |r| r.get(0))?;
        let delay_ms = (30_000i64 * 2i64.pow((attempts as u32).min(7))).min(3_600_000);
        c.execute("UPDATE outbox SET attempts = attempts + 1, next_attempt_at = ?2, error = ?3 WHERE id = ?1", params![id, now_ts() + delay_ms, error])?;
        Ok(())
    })
}

/// Drop every write still waiting: the user chose not to send them, so the
/// mailbox keeps what it has and Riddle keeps its own decisions.
pub fn discard_pending(store: &Store) -> Result<u64> {
    store.write(|c| {
        c.execute("DELETE FROM outbox WHERE applied_at IS NULL", [])
            .map(|n| n as u64)
    })
}

pub fn pending_count(store: &Store) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM outbox WHERE applied_at IS NULL",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as u64)
    })
}

/// Remove pending rows for a message (undo before projection ran).
pub fn cancel_for_message(store: &Store, message_id: &str) -> Result<u64> {
    store.write(|c| {
        Ok(c.execute(
            "DELETE FROM outbox WHERE applied_at IS NULL AND message_id = ?1",
            [message_id],
        )? as u64)
    })
}

/// Rows applied within the last `since_ms` for a message: the projection
/// echoes sync must not mistake for a human move (§15 attribution).
pub fn recently_applied_for_message<T: DeserializeOwned>(
    store: &Store,
    message_id: &str,
    since_ms: i64,
) -> Result<Vec<OutboxRow<T>>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT id, thread_id, message_id, op, json, attempts, idempotency_key FROM outbox WHERE message_id = ?1 AND applied_at IS NOT NULL AND error IS NULL AND applied_at >= ?2 ORDER BY applied_at DESC",
        )?;
        let rows = st.query_map(params![message_id, now_ts() - since_ms], |r| {
            Ok(OutboxRow {
                id: r.get(0)?,
                thread_id: r.get(1)?,
                message_id: r.get(2)?,
                op: r.get(3)?,
                payload: from_json(r.get(4)?)?,
                attempts: r.get::<_, i64>(5)? as u32,
                idempotency_key: r.get(6)?,
            })
        })?.collect();
        rows
    })
}

/// Pending rows for a message, oldest first (undo needs to know what is queued).
pub fn pending_for_message<T: DeserializeOwned>(
    store: &Store,
    message_id: &str,
) -> Result<Vec<OutboxRow<T>>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id, thread_id, message_id, op, json, attempts, idempotency_key FROM outbox WHERE message_id = ?1 AND applied_at IS NULL ORDER BY id")?;
        let rows = st.query_map([message_id], |r| {
            Ok(OutboxRow {
                id: r.get(0)?,
                thread_id: r.get(1)?,
                message_id: r.get(2)?,
                op: r.get(3)?,
                payload: from_json(r.get(4)?)?,
                attempts: r.get::<_, i64>(5)? as u32,
                idempotency_key: r.get(6)?,
            })
        })?.collect();
        rows
    })
}

/// Close a row without applying it, keeping the reason: a conflict where the
/// provider changed underneath (§15), or an op made moot by a later one.
pub fn mark_skipped(store: &Store, id: i64, reason: &str) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE outbox SET applied_at = ?2, error = ?3 WHERE id = ?1",
            params![id, now_ts(), reason],
        )?;
        Ok(())
    })
}

/// Rows that failed permanently (skipped with a reason), newest first.
pub fn recent_errors(store: &Store, limit: u32) -> Result<Vec<(i64, Option<String>, String)>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id, message_id, error FROM outbox WHERE error IS NOT NULL ORDER BY id DESC LIMIT ?1")?;
        let rows = st.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect();
        rows
    })
}
