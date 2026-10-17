//! The Signal Registry (index store) and Signal evaluations (Dig store).

use crate::{from_json, to_json, ts, Store};
use riddle_core::{MessageId, Signal, SignalEvaluation, SignalStatus};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, s: &Signal) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO signals (id, name, version, status, json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(name, version) DO UPDATE SET status = excluded.status, json = excluded.json",
            params![s.id.as_str(), s.name, s.version, s.status.as_str(), to_json(s)?, ts(&s.created_at)],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, name: &str, version: u32) -> Result<Option<Signal>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM signals WHERE name = ?1 AND version = ?2",
            params![name, version],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

/// The highest version of a Signal in one of the given statuses.
pub fn latest(store: &Store, name: &str, statuses: &[SignalStatus]) -> Result<Option<Signal>> {
    let all = versions(store, name)?;
    Ok(all
        .into_iter()
        .filter(|s| statuses.is_empty() || statuses.contains(&s.status))
        .max_by_key(|s| s.version))
}

pub fn versions(store: &Store, name: &str) -> Result<Vec<Signal>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM signals WHERE name = ?1 ORDER BY version")?;
        let rows = st
            .query_map([name], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn list(store: &Store) -> Result<Vec<Signal>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM signals ORDER BY name, version")?;
        let rows = st
            .query_map([], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn next_version(store: &Store, name: &str) -> Result<u32> {
    store.read(|c| {
        let v: Option<u32> = c.query_row(
            "SELECT MAX(version) FROM signals WHERE name = ?1",
            [name],
            |r| r.get(0),
        )?;
        Ok(v.unwrap_or(0) + 1)
    })
}

// ---- evaluations (Dig store) ----

pub fn put_evaluation(store: &Store, e: &SignalEvaluation) -> Result<()> {
    store.write(|c| put_evaluation_in(c, e))
}

pub fn put_evaluation_in(c: &rusqlite::Connection, e: &SignalEvaluation) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO signal_evaluations (message_id, signal_name, signal_version, engine_identity, context_hash, json, evaluated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            e.message_id.as_str(), e.signal_name, e.signal_version, e.engine_identity,
            e.context_hash.clone().unwrap_or_default(), to_json(e)?, ts(&e.evaluated_at)
        ],
    )?;
    Ok(())
}

pub fn get_evaluation(
    store: &Store,
    message: &MessageId,
    name: &str,
    version: u32,
    engine: &str,
    context_hash: &str,
) -> Result<Option<SignalEvaluation>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM signal_evaluations WHERE message_id = ?1 AND signal_name = ?2 AND signal_version = ?3 AND engine_identity = ?4 AND context_hash = ?5",
            params![message.as_str(), name, version, engine, context_hash],
            |r| from_json(r.get::<_, String>(0)?),
        ).optional()
    })
}

/// Every cached evaluation for a message, any engine.
pub fn evaluations_for(store: &Store, message: &MessageId) -> Result<Vec<SignalEvaluation>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM signal_evaluations WHERE message_id = ?1 ORDER BY evaluated_at DESC",
        )?;
        let rows = st
            .query_map([message.as_str()], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn evaluation_count(store: &Store) -> Result<u64> {
    store.read(|c| {
        c.query_row("SELECT count(*) FROM signal_evaluations", [], |r| {
            r.get::<_, i64>(0)
        })
        .map(|n| n as u64)
    })
}

/// Drop every cached evaluation for an engine identity (model change, §26).
pub fn invalidate_engine(store: &Store, engine: &str) -> Result<u64> {
    store.write(|c| {
        Ok(c.execute(
            "DELETE FROM signal_evaluations WHERE engine_identity = ?1",
            [engine],
        )? as u64)
    })
}

/// Cached evaluations for one message restricted to one engine identity;
/// simulation and the cache read this to avoid mixing engines.
pub fn evaluations_for_engine(
    store: &Store,
    message: &MessageId,
    engine: &str,
) -> Result<Vec<SignalEvaluation>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM signal_evaluations WHERE message_id = ?1 AND engine_identity = ?2 ORDER BY evaluated_at DESC")?;
        let rows = st.query_map(params![message.as_str(), engine], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// The evaluations of one Signal version for some messages, any engine, in
/// chunks so the statement stays small: what the sender review reads for
/// each sender's newest few messages, instead of the Dig's newest thousands.
pub fn evaluations_of_signal_for_ids(
    store: &Store,
    name: &str,
    version: u32,
    ids: &[MessageId],
) -> Result<Vec<SignalEvaluation>> {
    let mut out = Vec::new();
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT json FROM signal_evaluations WHERE signal_name = ?1 AND signal_version = ?2 AND message_id IN ({marks})"
        );
        let mut values: Vec<rusqlite::types::Value> =
            vec![name.to_string().into(), i64::from(version).into()];
        values.extend(chunk.iter().map(|id| id.as_str().to_string().into()));
        let rows: Vec<SignalEvaluation> = store.read(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st
                .query_map(rusqlite::params_from_iter(values), |r| {
                    from_json(r.get::<_, String>(0)?)
                })?
                .collect();
            rows
        })?;
        out.extend(rows);
    }
    Ok(out)
}

/// Every evaluation of one Signal version in the Dig, for calibration and
/// Signal testing.
pub fn evaluations_of_signal(
    store: &Store,
    name: &str,
    version: u32,
    limit: u32,
) -> Result<Vec<SignalEvaluation>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM signal_evaluations WHERE signal_name = ?1 AND signal_version = ?2 ORDER BY evaluated_at DESC LIMIT ?3")?;
        let rows = st.query_map(params![name, version, limit], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}
