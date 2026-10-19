//! Current and historical classifications in a Dig store.
//!
//! `messages.sifted_at` and `messages.shadow_sifted_at` mirror a message's
//! current classification through triggers on this table (migration 0008):
//! every insert, supersede or delete here updates them in the same
//! transaction, so the writers below carry no bookkeeping for the backlog.

use crate::{from_json, now_ts, to_json, ts, Store};
use riddle_core::{Classification, Disposition, DispositionKind, MessageId, ThreadId};
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
    let find_type = match &k.disposition {
        Disposition::Find { find_type } => Some(find_type.as_str()),
        _ => None,
    };
    c.execute(
        "INSERT INTO classifications (id, message_id, disposition_kind, find_type, mesh_id, mesh_version, json, shadow, created_at, series_key, importance)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            k.id.as_str(), k.message_id.as_str(), k.disposition.kind().as_str(), find_type,
            k.mesh_id.as_ref().map(|m| m.as_str()), k.mesh_version, to_json(k)?, k.shadow as i64, ts(&k.created_at),
            k.series.as_deref(), k.importance.rank()
        ],
    )?;
    if !k.shadow {
        c.execute(
            "DELETE FROM message_strata WHERE message_id = ?1",
            [k.message_id.as_str()],
        )?;
        for s in &k.strata {
            c.execute(
                "INSERT OR IGNORE INTO message_strata (message_id, stratum) VALUES (?1, ?2)",
                params![k.message_id.as_str(), s],
            )?;
        }
    }
    Ok(())
}

/// Retire every current decision Riddle made, not the user's, so the next
/// Sift pass decides those messages again. Returns how many were retired.
pub fn supersede_sift_decisions(store: &Store) -> Result<usize> {
    store.write(|c| {
        c.execute(
            "UPDATE classifications SET superseded_at = ?1
             WHERE shadow = 0 AND superseded_at IS NULL
               AND json_extract(json, '$.mechanism') != 'user'",
            params![crate::now_ts()],
        )
    })
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
        let mut st = c.prepare(
            "SELECT json FROM classifications WHERE message_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = st
            .query_map([id.as_str()], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// Counts of current dispositions, optionally within a window.
pub fn counts(
    store: &Store,
    since: Option<&chrono::DateTime<chrono::Utc>>,
    shadow: bool,
) -> Result<HashMap<DispositionKind, u64>> {
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

/// Current classifications for a set of messages in one query, for
/// simulation diffs and the resync.
pub fn current_many(
    store: &Store,
    ids: &[MessageId],
) -> Result<HashMap<MessageId, Classification>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let json = crate::ids_json(ids.iter().map(|id| id.as_str()));
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT message_id, json FROM classifications
             WHERE shadow = 0 AND superseded_at IS NULL AND message_id IN (SELECT value FROM json_each(?1))",
        )?;
        let rows = st
            .query_map([json], |r| {
                Ok((
                    MessageId::from(r.get::<_, String>(0)?),
                    from_json::<Classification>(r.get::<_, String>(1)?)?,
                ))
            })?
            .collect();
        rows
    })
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

/// Supersede the current (non-shadow) classification of one message without
/// writing a replacement: the message returns to the Sift backlog. Used by
/// undo when the action being undone was the message's first classification.
pub fn supersede_current(store: &Store, id: &MessageId) -> Result<Option<Classification>> {
    let prior = current(store, id)?;
    if prior.is_some() {
        store.write(|c| {
            c.execute(
                "UPDATE classifications SET superseded_at = ?2 WHERE message_id = ?1 AND shadow = 0 AND superseded_at IS NULL",
                params![id.as_str(), now_ts()],
            )?;
            c.execute("DELETE FROM message_strata WHERE message_id = ?1", [id.as_str()])?;
            Ok(())
        })?;
    }
    Ok(prior)
}

/// Current non-shadow classifications created by a Mesh version, for
/// simulation and undo previews.
/// Every current decision a Mesh made on this Dig, for reading them again.
pub fn current_mesh_decisions(store: &Store) -> Result<Vec<Classification>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM classifications WHERE mesh_id IS NOT NULL AND superseded_at IS NULL AND shadow = 0")?;
        let rows = st
            .query_map([], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// Rewrite one classification in place (its reason, say). The row keeps its
/// place in history; nothing is superseded.
pub fn update(store: &Store, k: &Classification) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE classifications SET json = ?2, importance = ?3 WHERE id = ?1",
            params![k.id.as_str(), to_json(k)?, k.importance.rank()],
        )
        .map(|_| ())
    })
}

pub fn current_by_mesh(store: &Store, mesh_id: &str, version: u32) -> Result<Vec<Classification>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM classifications WHERE mesh_id = ?1 AND mesh_version = ?2 AND superseded_at IS NULL AND shadow = 0")?;
        let rows = st.query_map(params![mesh_id, version], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// The newest current decisions a Mesh made (any version), for spot checks.
pub fn recent_by_mesh(store: &Store, mesh_id: &str, limit: u32) -> Result<Vec<Classification>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM classifications WHERE mesh_id = ?1 AND superseded_at IS NULL AND shadow = 0 ORDER BY created_at DESC LIMIT ?2")?;
        let rows = st.query_map(params![mesh_id, limit], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// How many current decisions a Mesh made (any version).
pub fn count_by_mesh(store: &Store, mesh_id: &str) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM classifications WHERE mesh_id = ?1 AND superseded_at IS NULL AND shadow = 0",
            [mesh_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as u64)
    })
}

/// Message ids whose current classification has the given disposition kind,
/// newest first, for building Signal test corpora (§31).
pub fn ids_with_kind(store: &Store, kind: DispositionKind, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT message_id FROM classifications WHERE disposition_kind = ?1 AND superseded_at IS NULL AND shadow = 0 ORDER BY created_at DESC LIMIT ?2")?;
        let rows = st.query_map(params![kind.as_str(), limit], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}

/// Rename a Stratum on every message that carries it (rename or merge of
/// Strata, §2). Returns how many message rows changed.
pub fn rename_stratum(store: &Store, from: &str, to: &str) -> Result<u64> {
    store.write(|c| {
        c.execute("INSERT OR IGNORE INTO message_strata (message_id, stratum) SELECT message_id, ?2 FROM message_strata WHERE stratum = ?1", params![from, to])?;
        Ok(c.execute("DELETE FROM message_strata WHERE stratum = ?1", [from])? as u64)
    })
}

/// Sift's decisions since a time (non-shadow, not the user's own): the total
/// and how many were Spoil. Aggregated in SQL; the trust card asks every
/// minute and must not parse every row to count them.
pub fn sift_totals_since(
    store: &Store,
    after: &chrono::DateTime<chrono::Utc>,
) -> Result<(u64, u64)> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*), coalesce(sum(disposition_kind = 'spoil'), 0) FROM classifications
             WHERE created_at > ?1 AND shadow = 0 AND json_extract(json, '$.mechanism') != 'user'",
            [ts(after)],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )
    })
}

/// Sift's decisions since a time, per deciding Mesh version.
pub fn decisions_by_mesh_since(
    store: &Store,
    after: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, u32, u64)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT mesh_id, mesh_version, count(*) FROM classifications
             WHERE created_at > ?1 AND shadow = 0 AND mesh_id IS NOT NULL AND json_extract(json, '$.mechanism') != 'user'
             GROUP BY mesh_id, mesh_version",
        )?;
        let rows = st
            .query_map([ts(after)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32, r.get::<_, i64>(2)? as u64)))?
            .collect();
        rows
    })
}

/// The message ids of Sift's latest decisions of one kind, newest first, for
/// streaks such as "the last 40 Spoil calls".
pub fn recent_sift_ids_of_kind(
    store: &Store,
    kind: DispositionKind,
    limit: u32,
) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT message_id FROM classifications
             WHERE disposition_kind = ?1 AND shadow = 0 AND json_extract(json, '$.mechanism') != 'user'
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = st
            .query_map(params![kind.as_str(), limit], |r| Ok(MessageId(r.get::<_, String>(0)?)))?
            .collect();
        rows
    })
}

/// Messages whose current decision is the person's own, newest first: the
/// right answers a model comparison scores against.
pub fn user_decided(store: &Store, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT message_id FROM classifications
             WHERE superseded_at IS NULL AND shadow = 0 AND json_extract(json, '$.mechanism') = 'user'
             ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = st
            .query_map([limit], |r| Ok(MessageId(r.get::<_, String>(0)?)))?
            .collect();
        rows
    })
}

/// The hard cases: messages the LLM decided or left Unresolved, newest
/// first. Where escalation matters, so where models are worth comparing.
pub fn recent_hard(store: &Store, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT message_id FROM classifications
             WHERE superseded_at IS NULL AND shadow = 0
               AND (json_extract(json, '$.mechanism') = 'llm' OR disposition_kind = 'unresolved')
             ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = st
            .query_map([limit], |r| Ok(MessageId(r.get::<_, String>(0)?)))?
            .collect();
        rows
    })
}
/// Adds Strata to the user's own replies in a thread, when a message in it
/// is decided with Strata they lack. A reply copies its thread's Strata
/// when it is decided, but Sift decides newest first, so a reply was often
/// decided before the message it answers and kept none.
pub fn spread_to_replies(store: &Store, thread: &ThreadId, strata: &[String]) -> Result<()> {
    if strata.is_empty() {
        return Ok(());
    }
    store.write(|c| {
        let rows: Vec<(String, String)> = {
            let mut st = c.prepare(
                "SELECT k.id, k.json FROM classifications k JOIN messages m ON m.id = k.message_id
                 WHERE m.thread_id = ?1 AND m.is_sent = 1 AND k.shadow = 0 AND k.superseded_at IS NULL",
            )?;
            let rows = st.query_map([thread.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        for (id, json) in rows {
            let mut k: Classification = from_json(json)?;
            let missing: Vec<&String> = strata.iter().filter(|s| !k.strata.contains(s)).collect();
            if missing.is_empty() {
                continue;
            }
            for s in missing {
                k.strata.push(s.clone());
                c.execute(
                    "INSERT OR IGNORE INTO message_strata (message_id, stratum) VALUES (?1, ?2)",
                    params![k.message_id.as_str(), s],
                )?;
            }
            c.execute("UPDATE classifications SET json = ?2 WHERE id = ?1", params![id, to_json(&k)?])?;
        }
        Ok(())
    })
}

/// Retires a message's Shadow proposal: the live decision was made again
/// and nothing in Shadow would decide it differently any more.
pub fn retire_shadow(store: &Store, id: &MessageId) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE classifications SET superseded_at = ?2 WHERE message_id = ?1 AND shadow = 1 AND superseded_at IS NULL",
            params![id.as_str(), now_ts()],
        )?;
        Ok(())
    })
}

/// Unresolved messages Riddle decided (not the user) under rules older than
/// `rules_changed_at`, that ran out of escalation budget before
/// `budget_reset_at`, or that were decided without a judge (the trace says
/// so: no key yet, a blip, an open breaker) before
/// `judge_lost_before`, which the caller passes only while a judge
/// answers: worth deciding again. Oldest first. A judge blip stranded
/// messages as Unresolved until the next rule edit (19 Oct 2026).
pub fn stale_unresolved(
    store: &Store,
    rules_changed_at: Option<&chrono::DateTime<chrono::Utc>>,
    budget_reset_at: &chrono::DateTime<chrono::Utc>,
    judge_lost_before: Option<&chrono::DateTime<chrono::Utc>>,
    limit: u32,
) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT message_id FROM classifications
             WHERE superseded_at IS NULL AND shadow = 0 AND disposition_kind = 'unresolved'
               AND coalesce(json_extract(json, '$.mechanism'), '') <> 'user'
               AND (created_at < ?1
                    OR (instr(json_extract(json, '$.reason'), ?4) > 0 AND created_at < ?2)
                    OR (created_at < ?5 AND EXISTS (
                        SELECT 1 FROM decision_traces t
                        WHERE t.id = json_extract(classifications.json, '$.traceId')
                          AND json_extract(t.json, '$.strategy') = ?6)))
             ORDER BY created_at LIMIT ?3",
        )?;
        let rows = st
            .query_map(
                params![
                    rules_changed_at.map(ts).unwrap_or(0),
                    ts(budget_reset_at),
                    limit,
                    riddle_core::BUDGET_EXHAUSTED,
                    judge_lost_before.map(ts).unwrap_or(0),
                    riddle_core::JUDGE_UNAVAILABLE_STRATEGY,
                ],
                |r| Ok(MessageId::from(r.get::<_, String>(0)?)),
            )?
            .collect();
        rows
    })
}

/// `retire_shadow` inside a caller's transaction: Sift persists a decision
/// (facts, classification, trace, series) as one write, where it was five
/// to seven (17 Oct 2026).
pub fn retire_shadow_in(c: &rusqlite::Connection, id: &MessageId) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE classifications SET superseded_at = ?2 WHERE message_id = ?1 AND shadow = 1 AND superseded_at IS NULL",
        params![id.as_str(), now_ts()],
    )?;
    Ok(())
}

/// `spread_to_replies` inside a caller's transaction (see `retire_shadow_in`).
pub fn spread_to_replies_in(
    c: &rusqlite::Connection,
    thread: &ThreadId,
    strata: &[String],
) -> rusqlite::Result<()> {
    if strata.is_empty() {
        return Ok(());
    }
    let rows: Vec<(String, String)> = {
        let mut st = c.prepare(
            "SELECT k.id, k.json FROM classifications k JOIN messages m ON m.id = k.message_id
             WHERE m.thread_id = ?1 AND m.is_sent = 1 AND k.shadow = 0 AND k.superseded_at IS NULL",
        )?;
        let rows = st
            .query_map([thread.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    for (id, json) in rows {
        let mut k: Classification = from_json(json)?;
        let missing: Vec<&String> = strata.iter().filter(|s| !k.strata.contains(s)).collect();
        if missing.is_empty() {
            continue;
        }
        for s in missing {
            k.strata.push(s.clone());
            c.execute(
                "INSERT OR IGNORE INTO message_strata (message_id, stratum) VALUES (?1, ?2)",
                params![k.message_id.as_str(), s],
            )?;
        }
        c.execute(
            "UPDATE classifications SET json = ?2 WHERE id = ?1",
            params![id, to_json(&k)?],
        )?;
    }
    Ok(())
}
