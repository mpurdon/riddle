use crate::{from_json, now_ts, to_json, Store};
use riddle_core::{Dig, DigGroup, DigId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, dig: &Dig) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO digs (id, json, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![dig.id.as_str(), to_json(dig)?, now_ts()],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &DigId) -> Result<Option<Dig>> {
    store.read(|c| {
        c.query_row("SELECT json FROM digs WHERE id = ?1", [id.as_str()], |r| {
            from_json(r.get::<_, String>(0)?)
        })
        .optional()
    })
}

pub fn list(store: &Store) -> Result<Vec<Dig>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM digs ORDER BY created_at")?;
        let rows = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?;
        rows.collect()
    })
}

/// The accounts Riddle works on: every one the user has not disabled. Work
/// that spans accounts reads this; only account management reads `list`.
pub fn list_enabled(store: &Store) -> Result<Vec<Dig>> {
    Ok(list(store)?.into_iter().filter(|d| !d.disabled).collect())
}

pub fn remove(store: &Store, id: &DigId) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM digs WHERE id = ?1", [id.as_str()])?;
        Ok(())
    })
}

pub fn upsert_group(store: &Store, g: &DigGroup) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT INTO dig_groups (name, json) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET json = excluded.json", params![g.name, to_json(g)?])?;
        Ok(())
    })
}

pub fn list_groups(store: &Store) -> Result<Vec<DigGroup>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM dig_groups ORDER BY name")?;
        let rows = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?;
        rows.collect()
    })
}

pub fn remove_group(store: &Store, name: &str) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM dig_groups WHERE name = ?1", [name])?;
        Ok(())
    })
}
