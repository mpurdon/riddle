//! The account's label catalogue: Gmail's ids with their names, refreshed
//! on every sync, and how many stored messages carry each.

use crate::{now_ts, Store};
use riddle_core::ProviderLabel;
use riddle_errors::Result;
use rusqlite::params;
use std::collections::HashMap;

/// Replace the catalogue with what the provider lists now; a label deleted
/// in Gmail drops out.
pub fn replace_all(store: &Store, labels: &[ProviderLabel]) -> Result<()> {
    // `write` runs in one transaction already: the list is replaced whole.
    store.write(|c| {
        c.execute("DELETE FROM provider_labels", [])?;
        let now = now_ts();
        for l in labels {
            c.execute(
                "INSERT OR REPLACE INTO provider_labels (id, name, system, managed, synced_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![l.id, l.name, l.system as i64, l.managed_by_riddle as i64, now],
            )?;
        }
        Ok(())
    })
}

pub fn all(store: &Store) -> Result<Vec<ProviderLabel>> {
    store.read(|c| {
        let mut st =
            c.prepare("SELECT id, name, system, managed FROM provider_labels ORDER BY name")?;
        let rows = st
            .query_map([], |r| {
                Ok(ProviderLabel {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    system: r.get::<_, i64>(2)? != 0,
                    managed_by_riddle: r.get::<_, i64>(3)? != 0,
                })
            })?
            .collect();
        rows
    })
}

/// Per label id: messages carrying it, and how many of those are unread.
pub fn counts(store: &Store) -> Result<HashMap<String, (u64, u64)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT j.value, count(*), sum(EXISTS (SELECT 1 FROM json_each(m.labels_json) u WHERE u.value = 'UNREAD'))
             FROM messages m, json_each(m.labels_json) j
             WHERE m.deleted_at IS NULL GROUP BY j.value",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64)))
            })?
            .collect();
        rows
    })
}
