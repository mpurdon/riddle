//! Pending (unconfirmed) actions and idempotency keys (§51, §52).

use crate::{from_json, from_ts, now_ts, to_json, Store};
use riddle_core::{ActivityId, PendingAction, ProposalOutcome, ProposalRecord};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

/// A new proposal: open in `pending_actions`, and recorded in `proposals`
/// until it resolves and after.
pub fn put_pending(store: &Store, p: &PendingAction) -> Result<()> {
    store.write(|c| {
        let now = now_ts();
        let json = to_json(p)?;
        c.execute(
            "INSERT OR REPLACE INTO pending_actions (id, json, created_at) VALUES (?1, ?2, ?3)",
            params![p.id, json, now],
        )?;
        c.execute(
            "INSERT OR REPLACE INTO proposals (id, conversation_id, action, outcome, created_at, json) VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
            params![p.id, p.conversation_id().map(|c| c.as_str()), p.action, now, json],
        )?;
        Ok(())
    })
}

/// A proposal leaves the pending list and what became of it is recorded,
/// in one transaction: the two used to be separate writes at every exit,
/// so a failure between them left a proposal gone from the list but
/// recorded as still pending. Deleting an already removed row is a no-op.
pub fn resolve(
    store: &Store,
    id: &str,
    outcome: ProposalOutcome,
    note: Option<String>,
    activity_id: Option<ActivityId>,
) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM pending_actions WHERE id = ?1", [id])?;
        c.execute(
            "UPDATE proposals SET outcome = ?2, note = ?3, activity_id = ?4, resolved_at = ?5 WHERE id = ?1",
            params![id, outcome.as_str(), note, activity_id.as_ref().map(|a| a.as_str()), now_ts()],
        )?;
        Ok(())
    })
}

/// Proposals newest first: one conversation's, or every one.
pub fn proposals(
    store: &Store,
    conversation_id: Option<&str>,
    limit: u32,
) -> Result<Vec<ProposalRecord>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json, outcome, note, activity_id, resolved_at FROM proposals WHERE (?1 IS NULL OR conversation_id = ?1) ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = st
            .query_map(params![conversation_id, limit], |r| {
                let proposal: PendingAction = from_json(r.get::<_, String>(0)?)?;
                let outcome: String = r.get(1)?;
                Ok(ProposalRecord {
                    conversation_id: proposal.conversation_id().cloned(),
                    proposal,
                    outcome: serde_json::from_value(serde_json::Value::String(outcome))
                        .unwrap_or(ProposalOutcome::Pending),
                    note: r.get(2)?,
                    activity_id: r.get::<_, Option<String>>(3)?.map(ActivityId::from),
                    resolved_at: r.get::<_, Option<i64>>(4)?.map(from_ts),
                })
            })?
            .collect();
        rows
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

/// Pending actions older than the TTL are dropped and recorded as expired,
/// in one transaction; a stale confirmation must not fire hours later.
pub fn expire_pending(store: &Store, ttl_ms: i64) -> Result<u64> {
    let cutoff = now_ts() - ttl_ms;
    store.write(|c| {
        c.execute(
            "UPDATE proposals SET outcome = 'expired', resolved_at = ?2 WHERE id IN (SELECT id FROM pending_actions WHERE created_at < ?1)",
            params![cutoff, now_ts()],
        )?;
        Ok(c.execute("DELETE FROM pending_actions WHERE created_at < ?1", [cutoff])? as u64)
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
