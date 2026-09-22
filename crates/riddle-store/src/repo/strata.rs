use crate::{from_json, to_json, Store};
use riddle_core::{Stratum, StratumId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, s: &Stratum) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO strata (id, name, json, deleted_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET name = excluded.name, json = excluded.json, deleted_at = excluded.deleted_at",
            params![s.id.as_str(), s.name, to_json(s)?, s.deleted_at.as_ref().map(crate::ts)],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &StratumId) -> Result<Option<Stratum>> {
    store.read(|c| c.query_row("SELECT json FROM strata WHERE id = ?1", [id.as_str()], |r| from_json(r.get::<_, String>(0)?)).optional())
}

pub fn list(store: &Store, include_deleted: bool) -> Result<Vec<Stratum>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM strata ORDER BY name")?;
        let all: Vec<Stratum> = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?.collect::<rusqlite::Result<_>>()?;
        Ok(all.into_iter().filter(|s| include_deleted || s.deleted_at.is_none()).collect())
    })
}

pub fn by_name(store: &Store, name: &str) -> Result<Option<Stratum>> {
    Ok(list(store, false)?.into_iter().find(|s| s.name.eq_ignore_ascii_case(name)))
}
