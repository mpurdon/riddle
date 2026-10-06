use crate::{from_json, to_json, ts, Store};
use riddle_core::{Job, JobId, JobStatus, Priority};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

fn status_str(s: &JobStatus) -> &'static str {
    match s {
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Paused { .. } => "paused",
        JobStatus::Completed => "completed",
        JobStatus::Failed { .. } => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

fn prio_i(p: Priority) -> i64 {
    match p {
        Priority::Interactive => 0,
        Priority::Incoming => 1,
        Priority::CatchUp => 2,
        Priority::Projection => 3,
        Priority::Backfill => 4,
        Priority::Experiment => 5,
    }
}

pub fn upsert(store: &Store, j: &Job) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO jobs (id, kind, priority, status, dig_id, json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET status = excluded.status, priority = excluded.priority, json = excluded.json, updated_at = excluded.updated_at",
            params![j.id.as_str(), j.kind.as_str(), prio_i(j.priority), status_str(&j.status), j.dig_id.as_ref().map(|d| d.as_str()), to_json(j)?, ts(&j.created_at), ts(&j.updated_at)],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &JobId) -> Result<Option<Job>> {
    store.read(|c| {
        c.query_row("SELECT json FROM jobs WHERE id = ?1", [id.as_str()], |r| {
            from_json(r.get::<_, String>(0)?)
        })
        .optional()
    })
}

/// Non-terminal jobs, highest priority first, oldest first within a class.
pub fn active(store: &Store) -> Result<Vec<Job>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM jobs WHERE status IN ('queued','running','paused') ORDER BY priority, created_at")?;
        let rows = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// The newest job of any of `kinds` for one Dig, whatever its status.
pub fn latest_for_dig(
    store: &Store,
    dig: &riddle_core::DigId,
    kinds: &[&str],
) -> Result<Option<Job>> {
    if kinds.is_empty() {
        return Ok(None);
    }
    // One statement over every kind; three round trips per refresh added up.
    let placeholders = (0..kinds.len())
        .map(|i| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT json FROM jobs WHERE dig_id = ?1 AND kind IN ({placeholders}) ORDER BY created_at DESC LIMIT 1"
    );
    store.read(|c| {
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![&dig.0];
        for k in kinds {
            args.push(k);
        }
        c.query_row(&sql, args.as_slice(), |r| from_json(r.get::<_, String>(0)?))
            .optional()
    })
}

pub fn recent(store: &Store, limit: u32) -> Result<Vec<Job>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM jobs ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = st
            .query_map([limit], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// On startup, running jobs are from a previous process; mark them queued so
/// they resume from their checkpoint.
pub fn requeue_orphans(store: &Store) -> Result<u64> {
    let running = store.read(|c| {
        let mut st = c.prepare("SELECT json FROM jobs WHERE status = 'running'")?;
        let rows = st
            .query_map([], |r| from_json::<Job>(r.get::<_, String>(0)?))?
            .collect::<rusqlite::Result<Vec<_>>>();
        rows
    })?;
    let n = running.len() as u64;
    for mut j in running {
        j.status = JobStatus::Queued;
        j.updated_at = chrono::Utc::now();
        upsert(store, &j)?;
    }
    Ok(n)
}

pub fn prune_finished(store: &Store, older_than: &chrono::DateTime<chrono::Utc>) -> Result<u64> {
    store.write(|c| Ok(c.execute("DELETE FROM jobs WHERE status IN ('completed','cancelled','failed') AND updated_at < ?1", [ts(older_than)])? as u64))
}
