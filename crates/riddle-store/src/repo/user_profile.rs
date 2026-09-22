use crate::{from_json, to_json, Store};
use riddle_core::UserProfile;
use riddle_errors::Result;
use rusqlite::OptionalExtension;

pub fn get(store: &Store) -> Result<UserProfile> {
    Ok(store
        .read(|c| {
            c.query_row("SELECT json FROM user_profile WHERE id = 1", [], |r| {
                from_json(r.get::<_, String>(0)?)
            })
            .optional()
        })?
        .unwrap_or_default())
}

pub fn put(store: &Store, p: &UserProfile) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT INTO user_profile (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json", [to_json(p)?])?;
        Ok(())
    })
}
