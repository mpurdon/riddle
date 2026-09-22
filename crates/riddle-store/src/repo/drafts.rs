use crate::{from_json, now_ts, to_json, Store};
use riddle_core::Draft;
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, d: &Draft) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT INTO drafts (id, json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET json = excluded.json, updated_at = excluded.updated_at", params![d.id, to_json(d)?, now_ts()])?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &str) -> Result<Option<Draft>> {
    store.read(|c| {
        c.query_row("SELECT json FROM drafts WHERE id = ?1", [id], |r| {
            from_json(r.get::<_, String>(0)?)
        })
        .optional()
    })
}

pub fn list(store: &Store) -> Result<Vec<Draft>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM drafts ORDER BY updated_at DESC")?;
        let rows = st
            .query_map([], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn remove(store: &Store, id: &str) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM drafts WHERE id = ?1", [id])?;
        Ok(())
    })
}

pub fn snooze(
    store: &Store,
    message_id: &str,
    until: &chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO snoozes (message_id, until) VALUES (?1, ?2)",
            params![message_id, crate::ts(until)],
        )?;
        Ok(())
    })
}

pub fn unsnooze(store: &Store, message_id: &str) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM snoozes WHERE message_id = ?1", [message_id])?;
        Ok(())
    })
}

pub fn snoozes_due(store: &Store, now: &chrono::DateTime<chrono::Utc>) -> Result<Vec<String>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT message_id FROM snoozes WHERE until <= ?1")?;
        let rows = st.query_map([crate::ts(now)], |r| r.get(0))?.collect();
        rows
    })
}

pub fn snoozed_until(
    store: &Store,
    message_id: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    store.read(|c| {
        c.query_row(
            "SELECT until FROM snoozes WHERE message_id = ?1",
            [message_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map(|o| o.map(crate::from_ts))
    })
}
