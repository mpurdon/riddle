//! Where a message stands in its `+ Replaces` series (migration 0004): the
//! newest Spoil or Filed message of a series keeps its standing and the
//! older ones are superseded by it, by message date, so re-deciding history
//! in any order lands on the same newest. A message the user kept (undid its
//! superseding) is out of the running for good.

use crate::{ts, Store};
use riddle_core::MessageId;
use riddle_errors::Result;
use rusqlite::params;

/// The members of a series: current, applied Spoil or Filed decisions in it
/// that the user did not keep, with where each stands now.
const MEMBERS: &str = "SELECT k.message_id, m.date, s.superseded_by FROM classifications k
     JOIN messages m ON m.id = k.message_id AND m.deleted_at IS NULL
     LEFT JOIN message_series s ON s.message_id = k.message_id
     WHERE k.series_key = ?1 AND k.superseded_at IS NULL AND k.shadow = 0
       AND k.disposition_kind IN ('spoil', 'filed') AND s.kept_at IS NULL";

/// Settle one series. Returns the messages newly superseded. Writes only the
/// rows whose standing changed: settling after each new message touches the
/// new one and the previous newest, not the whole series.
pub fn settle(
    store: &Store,
    key: &str,
    now: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<MessageId>> {
    store.write(|c| settle_in(c, key, now))
}

/// The user undid a superseding: these messages stand again and stay out of
/// their series from now on.
pub fn keep(
    store: &Store,
    ids: &[MessageId],
    now: &chrono::DateTime<chrono::Utc>,
) -> Result<usize> {
    store.write(|c| {
        let mut n = 0;
        for id in ids {
            n += c.execute(
                "UPDATE message_series SET superseded_by = NULL, kept_at = ?2, updated_at = ?2
                 WHERE message_id = ?1 AND superseded_by IS NOT NULL",
                params![id.as_str(), ts(now)],
            )?;
        }
        Ok(n)
    })
}

/// `settle` inside a caller's transaction: Sift persists a decision and
/// settles its series in one write (17 Oct 2026).
pub fn settle_in(
    c: &rusqlite::Connection,
    key: &str,
    now: &chrono::DateTime<chrono::Utc>,
) -> rusqlite::Result<Vec<MessageId>> {
    let members: Vec<(String, i64, Option<String>)> = {
        let mut st = c.prepare(MEMBERS)?;
        let rows = st
            .query_map([key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let Some(keeper) = members
        .iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|m| m.0.clone())
    else {
        return Ok(vec![]);
    };
    let mut newly = Vec::new();
    for (id, _, before) in &members {
        let by = (*id != keeper).then(|| keeper.clone());
        if *before == by {
            continue;
        }
        c.execute(
            "INSERT INTO message_series (message_id, series_key, superseded_by, kept_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(message_id) DO UPDATE SET series_key = excluded.series_key,
               superseded_by = excluded.superseded_by, updated_at = excluded.updated_at",
            params![id, key, by, ts(now)],
        )?;
        if by.is_some() {
            newly.push(MessageId(id.clone()));
        }
    }
    // Messages that left the series (re-decided as something else) and
    // were not kept no longer stand in it.
    c.execute(
        &format!(
            "DELETE FROM message_series WHERE series_key = ?1 AND kept_at IS NULL
               AND message_id NOT IN (SELECT message_id FROM ({MEMBERS}))"
        ),
        [key],
    )?;
    Ok(newly)
}
