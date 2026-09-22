use crate::{from_ts, ts, Store};
use riddle_core::{UsageRow, UsageSummary, UsageTask};
use riddle_errors::Result;
use rusqlite::params;

pub fn record(store: &Store, u: &UsageRow) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO usage (id, at, provider, model, task, trace_id, input_tokens, output_tokens, cost_usd, latency_ms, ok) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![u.id, ts(&u.at), u.provider, u.model, u.task.as_str(), u.trace_id.as_ref().map(|t| t.as_str()), u.input_tokens as i64, u.output_tokens as i64, u.cost_usd, u.latency_ms as i64, u.ok as i64],
        )?;
        Ok(())
    })
}

pub fn summary(store: &Store, since: &chrono::DateTime<chrono::Utc>) -> Result<UsageSummary> {
    store.read(|c| {
        let (calls, input, output, priced, unpriced): (i64, i64, i64, f64, i64) = c.query_row(
            "SELECT count(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0), COALESCE(SUM(CASE WHEN cost_usd IS NULL THEN 1 ELSE 0 END),0) FROM usage WHERE at >= ?1",
            [ts(since)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        let mut st = c.prepare("SELECT task, COALESCE(SUM(cost_usd),0), count(*) FROM usage WHERE at >= ?1 GROUP BY task")?;
        let by_task = st.query_map([ts(since)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, i64>(2)? as u64)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut st = c.prepare("SELECT provider, COALESCE(SUM(cost_usd),0), count(*) FROM usage WHERE at >= ?1 GROUP BY provider")?;
        let by_provider = st.query_map([ts(since)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, i64>(2)? as u64)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(UsageSummary { since: *since, calls: calls as u64, input_tokens: input as u64, output_tokens: output as u64, priced_usd: priced, unpriced_calls: unpriced as u64, by_task, by_provider })
    })
}

/// Spend today in USD for one task class, for the daily budget.
pub fn spent_today(store: &Store, tasks: &[UsageTask]) -> Result<f64> {
    let start = chrono::Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    store.read(|c| {
        let mut total = 0.0;
        for t in tasks {
            let v: f64 = c.query_row("SELECT COALESCE(SUM(cost_usd),0) FROM usage WHERE at >= ?1 AND task = ?2", params![ts(&start), t.as_str()], |r| r.get(0))?;
            total += v;
        }
        Ok(total)
    })
}

pub fn recent(store: &Store, limit: u32) -> Result<Vec<UsageRow>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id, at, provider, model, task, trace_id, input_tokens, output_tokens, cost_usd, latency_ms, ok FROM usage ORDER BY at DESC LIMIT ?1")?;
        let rows = st.query_map([limit], |r| {
            let task: String = r.get(4)?;
            Ok(UsageRow {
                id: r.get(0)?,
                at: from_ts(r.get(1)?),
                provider: r.get(2)?,
                model: r.get(3)?,
                task: match task.as_str() { "sift" => UsageTask::Sift, "director" => UsageTask::Director, "expedition" => UsageTask::Expedition, "simulation" => UsageTask::Simulation, "backfill" => UsageTask::Backfill, "signal_test" => UsageTask::SignalTest, _ => UsageTask::Probe },
                dig_id: None,
                trace_id: r.get::<_, Option<String>>(5)?.map(riddle_core::TraceId),
                input_tokens: r.get::<_, i64>(6)? as u64,
                output_tokens: r.get::<_, i64>(7)? as u64,
                cost_usd: r.get(8)?,
                latency_ms: r.get::<_, i64>(9)? as u64,
                ok: r.get::<_, i64>(10)? != 0,
            })
        })?.collect();
        rows
    })
}

/// Spend today in USD for one or more providers (`jev`, `bedrock`), for the
/// per-Dig daily budgets (§56): the judge and the LLM are budgeted separately
/// whatever task used them.
pub fn spent_today_by_provider(store: &Store, providers: &[&str]) -> Result<f64> {
    let start = chrono::Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap_or_default().and_utc();
    store.read(|c| {
        let mut total = 0.0;
        for p in providers {
            let v: f64 = c.query_row("SELECT COALESCE(SUM(cost_usd),0) FROM usage WHERE at >= ?1 AND provider = ?2", params![ts(&start), p], |r| r.get(0))?;
            total += v;
        }
        Ok(total)
    })
}
