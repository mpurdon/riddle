//! Model comparison outputs (index store): one row per model per item,
//! recorded and never applied. The roster's access checks and the person's
//! Director picks live in kv (`comparison_model:<id>`, `comparison_pick:<key>`).

use crate::{from_json, to_json, ts, Store};
use riddle_core::{BenchOutput, BenchTask};
use riddle_errors::Result;
use rusqlite::params;

pub const MODEL_PREFIX: &str = "comparison_model:";
pub const PICK_PREFIX: &str = "comparison_pick:";

pub fn record(store: &Store, o: &BenchOutput) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO comparison_outputs (id, task, source, model_id, dig_id, message_id, question_key, json, cost_usd, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                o.id,
                o.task.as_str(),
                serde_json::to_value(o.source).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
                o.model_id,
                o.dig_id.as_ref().map(|d| d.as_str().to_string()),
                o.message_id.as_ref().map(|m| m.as_str().to_string()),
                o.question_key,
                to_json(o)?,
                o.cost_usd,
                ts(&o.created_at)
            ],
        )
        .map(|_| ())
    })
}

/// The newest outputs of a task, newest first.
pub fn recent(store: &Store, task: BenchTask, limit: u32) -> Result<Vec<BenchOutput>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM comparison_outputs WHERE task = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = st
            .query_map(params![task.as_str(), limit], |r| {
                from_json(r.get::<_, String>(0)?)
            })?
            .collect();
        rows
    })
}

/// What comparisons have spent since `since` (unpriced runs count as zero).
pub fn spent_since(store: &Store, since: &chrono::DateTime<chrono::Utc>) -> Result<f64> {
    store.read(|c| {
        c.query_row(
            "SELECT coalesce(sum(cost_usd), 0) FROM comparison_outputs WHERE created_at >= ?1",
            [ts(since)],
            |r| r.get(0),
        )
    })
}
