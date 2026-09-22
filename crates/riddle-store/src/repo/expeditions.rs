use crate::{from_json, to_json, ts, Store};
use riddle_core::{Expedition, ExpeditionId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, e: &Expedition) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO expeditions (id, status, json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET status = excluded.status, json = excluded.json, updated_at = excluded.updated_at",
            params![e.id.as_str(), format!("{:?}", e.status).to_lowercase(), to_json(e)?, ts(&e.created_at), ts(&e.updated_at)],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &ExpeditionId) -> Result<Option<Expedition>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM expeditions WHERE id = ?1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

pub fn list(store: &Store, limit: u32) -> Result<Vec<Expedition>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM expeditions ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = st
            .query_map([limit], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn resumable(store: &Store) -> Result<Vec<Expedition>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM expeditions WHERE status IN ('running','paused','planned') ORDER BY created_at")?;
        let rows = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}
