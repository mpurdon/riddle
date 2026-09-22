//! Current and historical classifications in a Dig store.

use crate::{from_json, now_ts, to_json, ts, Store};
use riddle_core::{Classification, Disposition, DispositionKind, MessageId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

/// Insert a new classification and supersede the previous current one for
/// the same message and shadow flag. Also maintains `message_strata`.
pub fn record(store: &Store, k: &Classification) -> Result<()> {
    store.write(|c| record_in(c, k))
}

pub fn record_in(c: &rusqlite::Connection, k: &Classification) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE classifications SET superseded_at = ?2 WHERE message_id = ?1 AND shadow = ?3 AND superseded_at IS NULL",
        params![k.message_id.as_str(), ts(&k.created_at), k.shadow as i64],
    )?;
    let find_type = match &k.disposition { Disposition::Find { find_type } => Some(find_type.as_str()), _ => None };
    c.execute(
        "INSERT INTO classifications (id, message_id, disposition_kind, find_type, mesh_id, mesh_version, json, shadow, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            k.id.as_str(), k.message_id.as_str(), k.disposition.kind().as_str(), find_type,
            k.mesh_id.as_ref().map(|m| m.as_str()), k.mesh_version, to_json(k)?, k.shadow as i64, ts(&k.created_at)
        ],
    )?;
    if !k.shadow {
        c.execute("DELETE FROM message_strata WHERE message_id = ?1", [k.message_id.as_str()])?;
        for s in &k.strata {
            c.execute("INSERT OR IGNORE INTO message_strata (message_id, stratum) VALUES (?1, ?2)", params![k.message_id.as_str(), s])?;
        }
    }
    Ok(())
}

pub fn current(store: &Store, id: &MessageId) -> Result<Option<Classification>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM classifications WHERE message_id = ?1 AND shadow = 0 AND superseded_at IS NULL",
            [id.as_str()], |r| from_json(r.get::<_, String>(0)?),
        ).optional()
    })
}

pub fn current_shadow(store: &Store, id: &MessageId) -> Result<Option<Classification>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM classifications WHERE message_id = ?1 AND shadow = 1 AND superseded_at IS NULL",
            [id.as_str()], |r| from_json(r.get::<_, String>(0)?),
        ).optional()
    })
}

pub fn history(store: &Store, id: &MessageId) -> Result<Vec<Classification>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM classifications WHERE message_id = ?1 ORDER BY created_at DESC")?;
        let rows = st.query_map([id.as_str()], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// Counts of current dispositions, optionally within a window.
pub fn counts(store: &Store, since: Option<&chrono::DateTime<chrono::Utc>>, shadow: bool) -> Result<HashMap<DispositionKind, u64>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT disposition_kind, count(*) FROM classifications WHERE superseded_at IS NULL AND shadow = ?1 AND created_at >= ?2 GROUP BY disposition_kind",
        )?;
        let since_ts = since.map(ts).unwrap_or(0);
        let rows = st.query_map(params![shadow as i64, since_ts], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (k, n) = row?;
            let kind = DispositionKind::ALL.iter().copied().find(|d| d.as_str() == k);
            if let Some(kind) = kind { out.insert(kind, n); }
        }
        Ok(out)
    })
}

/// Current classifications for a set of messages, for simulation diffs.
pub fn current_many(store: &Store, ids: &[MessageId]) -> Result<HashMap<MessageId, Classification>> {
    let mut out = HashMap::new();
    for id in ids {
        if let Some(k) = current(store, id)? { out.insert(id.clone(), k); }
    }
    Ok(out)
}

/// Mark every current classification produced by a Mesh version superseded,
/// so the messages return to the backlog (undo of a Mesh change).
pub fn supersede_by_mesh(store: &Store, mesh_id: &str, version: u32) -> Result<Vec<MessageId>> {
    store.write(|c| {
        let mut st = c.prepare("SELECT message_id FROM classifications WHERE mesh_id = ?1 AND mesh_version = ?2 AND superseded_at IS NULL")?;
        let ids: Vec<MessageId> = st.query_map(params![mesh_id, version], |r| Ok(MessageId(r.get(0)?)))?.collect::<rusqlite::Result<_>>()?;
        c.execute("UPDATE classifications SET superseded_at = ?3 WHERE mesh_id = ?1 AND mesh_version = ?2 AND superseded_at IS NULL", params![mesh_id, version, now_ts()])?;
        Ok(ids)
    })
}
