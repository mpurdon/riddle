//! The account's label catalogue: Gmail's ids with their names, listed on
//! every sync and written only when the listing changed, and how many
//! stored messages carry each. `replace_all` records a hash of what it
//! wrote in `sync_state` (`LISTING_HASH`) in the same transaction, so the
//! sync compares the provider's listing against what the table holds,
//! whoever last wrote it: the catalogue was deleted and reinserted on
//! every poll (17 Oct 2026).

use crate::{now_ts, Store};
use riddle_core::ProviderLabel;
use riddle_errors::Result;
use rusqlite::params;
use std::collections::HashMap;

/// The `sync_state` key holding the hash of the listing last written.
pub const LISTING_HASH: &str = "labels_hash";

/// A stable, order-free hash of a label listing.
pub fn listing_hash(labels: &[ProviderLabel]) -> String {
    let mut rows: Vec<String> = labels
        .iter()
        .map(|l| {
            format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                l.id, l.name, l.system, l.managed_by_riddle
            )
        })
        .collect();
    rows.sort();
    riddle_core::sha256_hex(rows.join("\n"))
}

/// Replace the catalogue with what the provider lists now; a label deleted
/// in Gmail drops out.
pub fn replace_all(store: &Store, labels: &[ProviderLabel]) -> Result<()> {
    // `write` runs in one transaction already: the list is replaced whole.
    let hash = listing_hash(labels);
    store.write(|c| {
        c.execute("DELETE FROM provider_labels", [])?;
        let now = now_ts();
        for l in labels {
            c.execute(
                "INSERT OR REPLACE INTO provider_labels (id, name, system, managed, synced_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![l.id, l.name, l.system as i64, l.managed_by_riddle as i64, now],
            )?;
        }
        c.execute(
            "INSERT INTO sync_state (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![LISTING_HASH, hash, now],
        )?;
        Ok(())
    })
}

/// Whether the catalogue already holds this listing.
pub fn holds(store: &Store, labels: &[ProviderLabel]) -> Result<bool> {
    Ok(
        crate::repo::sync_state::get(store, LISTING_HASH)?.as_deref()
            == Some(listing_hash(labels).as_str()),
    )
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
