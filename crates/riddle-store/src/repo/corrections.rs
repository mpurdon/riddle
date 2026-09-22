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
        let mut st = c.prepare("SELECT json FROM corrections WHERE message_id = ?1 ORDER BY created_at DESC")?;
        let rows = st.query_map([id.as_str()], |r| from_json(r.get::<_, String>(0)?))?.collect();
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
        let rows = st.query_map([], |r| from_json::<Correction>(r.get::<_, String>(0)?))?.collect::<rusqlite::Result<Vec<_>>>();
        rows
    })?;
    let mut n = 0;
    for mut k in all.into_iter().filter(|k| k.activity_id.as_ref() == Some(activity)) {
        k.retracted_at = Some(chrono::Utc::now());
        store.write(|c| { c.execute("UPDATE corrections SET json = ?2, retracted_at = ?3 WHERE id = ?1", params![k.id.as_str(), to_json(&k)?, now_ts()])?; Ok(()) })?;
        n += 1;
    }
    Ok(n)
}
