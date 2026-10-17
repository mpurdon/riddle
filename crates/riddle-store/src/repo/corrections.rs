use crate::{from_json, now_ts, to_json, ts, Store};
use riddle_core::{ActivityId, Correction, MessageId};
use riddle_errors::Result;
use rusqlite::params;

pub fn record(store: &Store, k: &Correction) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO corrections (id, message_id, kind, json, created_at, retracted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![k.id.as_str(), k.message_id.as_str(), format!("{:?}", k.kind).to_lowercase(), to_json(k)?, ts(&k.created_at), k.retracted_at.as_ref().map(ts)],
        )?;
        Ok(())
    })
}

pub fn for_message(store: &Store, id: &MessageId) -> Result<Vec<Correction>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM corrections WHERE message_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = st
            .query_map([id.as_str()], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// Active (not retracted) corrections, newest first.
pub fn active(store: &Store, limit: u32) -> Result<Vec<Correction>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM corrections WHERE retracted_at IS NULL ORDER BY created_at DESC LIMIT ?1")?;
        let rows = st.query_map([limit], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// Retract the corrections an Activity generated (undo, §50).
pub fn retract_by_activity(store: &Store, activity: &ActivityId) -> Result<u64> {
    let all = store.read(|c| {
        let mut st = c.prepare("SELECT json FROM corrections WHERE retracted_at IS NULL")?;
        let rows = st
            .query_map([], |r| from_json::<Correction>(r.get::<_, String>(0)?))?
            .collect::<rusqlite::Result<Vec<_>>>();
        rows
    })?;
    let mut n = 0;
    for mut k in all
        .into_iter()
        .filter(|k| k.activity_id.as_ref() == Some(activity))
    {
        k.retracted_at = Some(chrono::Utc::now());
        store.write(|c| {
            c.execute(
                "UPDATE corrections SET json = ?2, retracted_at = ?3 WHERE id = ?1",
                params![k.id.as_str(), to_json(&k)?, now_ts()],
            )?;
            Ok(())
        })?;
        n += 1;
    }
    Ok(n)
}

/// Active corrections that changed the disposition of a message, newest
/// first, for Signal test corpora and the labelled calibration set (§31).
pub fn active_moves(store: &Store, limit: u32) -> Result<Vec<Correction>> {
    Ok(active(store, limit)?
        .into_iter()
        .filter(|k| k.to_disposition.is_some())
        .collect())
}

/// Counts per Mesh version: `(mesh id, version, count)`.
pub type MeshCounts = Vec<(String, u32, u64)>;

/// Corrections that still stand, made since a time: the total and the count
/// per Mesh version they were held against. Aggregated in SQL for the
/// trust card.
pub fn active_since_by_mesh(
    store: &Store,
    after: &chrono::DateTime<chrono::Utc>,
) -> Result<(u64, MeshCounts)> {
    store.read(|c| {
        let total: i64 = c.query_row(
            "SELECT count(*) FROM corrections WHERE retracted_at IS NULL AND created_at > ?1",
            [ts(after)],
            |r| r.get(0),
        )?;
        let mut st = c.prepare(
            "SELECT json_extract(json, '$.meshId'), json_extract(json, '$.meshVersion'), count(*) FROM corrections
             WHERE retracted_at IS NULL AND created_at > ?1 AND json_extract(json, '$.meshId') IS NOT NULL
             GROUP BY 1, 2",
        )?;
        let rows: rusqlite::Result<MeshCounts> = st
            .query_map([ts(after)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32, r.get::<_, i64>(2)? as u64)))?
            .collect();
        Ok((total as u64, rows?))
    })
}

/// Messages whose standing corrections reversed a Spoil decision.
pub fn reversed_from_spoil(store: &Store) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT DISTINCT message_id FROM corrections
             WHERE retracted_at IS NULL AND json_extract(json, '$.fromDisposition.kind') = 'spoil'",
        )?;
        let rows = st
            .query_map([], |r| Ok(MessageId(r.get::<_, String>(0)?)))?
            .collect();
        rows
    })
}

/// For one Mesh version on this Dig: how many distinct messages it decided
/// (still standing, or since reversed) and how many of those the user
/// reversed. Two aggregates, so a correction inside a bulk move costs two
/// counts rather than a parse of every correction and classification.
pub fn decided_and_reversed_by_mesh(
    store: &Store,
    mesh_id: &str,
    version: u32,
) -> Result<(usize, usize)> {
    store.read(|c| {
        let reversed: i64 = c.query_row(
            "SELECT count(DISTINCT message_id) FROM corrections
             WHERE retracted_at IS NULL AND json_extract(json, '$.meshId') = ?1 AND json_extract(json, '$.meshVersion') = ?2",
            params![mesh_id, version],
            |r| r.get(0),
        )?;
        let decided: i64 = c.query_row(
            "SELECT count(*) FROM (
                SELECT message_id FROM classifications WHERE mesh_id = ?1 AND mesh_version = ?2 AND superseded_at IS NULL AND shadow = 0
                UNION
                SELECT message_id FROM corrections WHERE retracted_at IS NULL AND json_extract(json, '$.meshId') = ?1 AND json_extract(json, '$.meshVersion') = ?2
             )",
            params![mesh_id, version],
            |r| r.get(0),
        )?;
        Ok((decided as usize, reversed as usize))
    })
}

/// A standing correction with where its message came from: the thread and
/// the sender, read in the same query. The Context slice that lists the
/// corrections touching a message's sender or thread read the fifty newest
/// corrections and then each corrected message, bodies and all, for every
/// message sifted (17 Oct 2026); it needs these two columns and nothing else.
#[derive(Debug, Clone)]
pub struct CorrectionOrigin {
    pub correction: Correction,
    /// `None` when the corrected message is gone (deleted or purged).
    pub thread_id: Option<riddle_core::ThreadId>,
    pub from_email: Option<String>,
}

/// Active (not retracted) corrections, newest first, each with its
/// message's thread and sender.
pub fn active_with_origin(store: &Store, limit: u32) -> Result<Vec<CorrectionOrigin>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT k.json, m.thread_id, m.from_email FROM corrections k
             LEFT JOIN messages m ON m.id = k.message_id AND m.deleted_at IS NULL
             WHERE k.retracted_at IS NULL ORDER BY k.created_at DESC LIMIT ?1",
        )?;
        let rows = st
            .query_map([limit], |r| {
                Ok(CorrectionOrigin {
                    correction: from_json(r.get::<_, String>(0)?)?,
                    thread_id: r.get::<_, Option<String>>(1)?.map(riddle_core::ThreadId),
                    from_email: r.get(2)?,
                })
            })?
            .collect();
        rows
    })
}
