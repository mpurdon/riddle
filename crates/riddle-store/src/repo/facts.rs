use crate::{from_json, now_ts, to_json, Store};
use riddle_core::{Facts, MessageId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn put(store: &Store, id: &MessageId, f: &Facts) -> Result<()> {
    store.write(|c| put_in(c, id, f))
}

pub fn put_in(c: &rusqlite::Connection, id: &MessageId, f: &Facts) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO facts (message_id, json, computed_at) VALUES (?1, ?2, ?3)",
        params![id.as_str(), to_json(f)?, now_ts()],
    )?;
    Ok(())
}

/// Every message with a stored facts row, for a refresh that walks them all.
pub fn ids(store: &Store) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT message_id FROM facts")?;
        let rows = st.query_map([], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}

pub fn get(store: &Store, id: &MessageId) -> Result<Option<Facts>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM facts WHERE message_id = ?1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}
