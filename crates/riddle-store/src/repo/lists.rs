//! Named lists (`riddle_core::lists`), one per key `list:<name>` in the
//! index kv table: a handful of small lists that every Mesh evaluation
//! reads, so no table of their own.

use crate::Store;
use riddle_core::{NamedListDef, NamedLists};
use riddle_errors::Result;

const PREFIX: &str = "list:";

pub fn get(store: &Store, name: &str) -> Result<Option<NamedListDef>> {
    crate::repo::kv::get_json(store, &format!("{PREFIX}{name}"))
}

pub fn all(store: &Store) -> Result<Vec<NamedListDef>> {
    Ok(crate::repo::kv::list_prefix(store, PREFIX)?
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str(&v).ok())
        .collect())
}

/// Every list by name, for evaluation.
pub fn named(store: &Store) -> Result<NamedLists> {
    Ok(NamedLists::from_defs(all(store)?))
}

pub fn put(store: &Store, list: &NamedListDef) -> Result<()> {
    crate::repo::kv::set_json(store, &format!("{PREFIX}{}", list.name), list)
}

pub fn delete(store: &Store, name: &str) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM kv WHERE key = ?1", [format!("{PREFIX}{name}")])?;
        Ok(())
    })
}
