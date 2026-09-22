use crate::{from_json, to_json, ts, Store};
use riddle_core::{DecisionTrace, MessageId, TraceId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn record(store: &Store, t: &DecisionTrace) -> Result<()> {
    store.write(|c| record_in(c, t))
}

pub fn record_in(c: &rusqlite::Connection, t: &DecisionTrace) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO decision_traces (id, message_id, json, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![t.id.as_str(), t.message_id.as_str(), to_json(t)?, ts(&t.created_at)],
    )?;
    Ok(())
}

pub fn get(store: &Store, id: &TraceId) -> Result<Option<DecisionTrace>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM decision_traces WHERE id = ?1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

pub fn for_message(store: &Store, id: &MessageId) -> Result<Vec<DecisionTrace>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM decision_traces WHERE message_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = st
            .query_map([id.as_str()], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn recent(store: &Store, limit: u32) -> Result<Vec<DecisionTrace>> {
    store.read(|c| {
        let mut st =
            c.prepare("SELECT json FROM decision_traces ORDER BY created_at DESC LIMIT ?1")?;
        let rows = st
            .query_map([limit], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// Aggregates for the Performance and Cost Observatories (§84, §87).
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct TraceStats {
    pub count: u64,
    pub escalated: u64,
    pub unresolved: u64,
    pub avg_total_ms: f64,
    pub avg_judge_ms: f64,
    pub judge_tokens: u64,
    pub judge_cost_usd: f64,
    pub llm_cost_usd: f64,
}

pub fn stats(store: &Store, since: &chrono::DateTime<chrono::Utc>) -> Result<TraceStats> {
    let traces = store.read(|c| {
        let mut st = c.prepare("SELECT json FROM decision_traces WHERE created_at >= ?1")?;
        let rows = st
            .query_map([ts(since)], |r| {
                from_json::<DecisionTrace>(r.get::<_, String>(0)?)
            })?
            .collect::<rusqlite::Result<Vec<_>>>();
        rows
    })?;
    let mut s = TraceStats {
        count: traces.len() as u64,
        ..Default::default()
    };
    if traces.is_empty() {
        return Ok(s);
    }
    let mut total = 0u64;
    let mut judge = 0u64;
    for t in &traces {
        if t.escalation.is_some() {
            s.escalated += 1;
        }
        if matches!(t.conclusion, riddle_core::Disposition::Unresolved) {
            s.unresolved += 1;
        }
        total += t.timing.total_ms;
        judge += t.timing.judge_ms;
        s.judge_tokens += t.cost.judge_input_tokens;
        s.judge_cost_usd += t.cost.judge_cost_usd.unwrap_or(0.0);
        s.llm_cost_usd += t.cost.llm_cost_usd.unwrap_or(0.0);
    }
    s.avg_total_ms = total as f64 / traces.len() as f64;
    s.avg_judge_ms = judge as f64 / traces.len() as f64;
    Ok(s)
}
