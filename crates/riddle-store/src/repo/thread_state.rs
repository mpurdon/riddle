use crate::{from_json, now_ts, to_json, Store};
use riddle_core::{ThreadId, ThreadState};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn put(store: &Store, s: &ThreadState) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT OR REPLACE INTO thread_state (thread_id, json, updated_at) VALUES (?1, ?2, ?3)", params![s.thread_id.as_str(), to_json(s)?, now_ts()])?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &ThreadId) -> Result<Option<ThreadState>> {
    store.read(|c| c.query_row("SELECT json FROM thread_state WHERE thread_id = ?1", [id.as_str()], |r| from_json(r.get::<_, String>(0)?)).optional())
}

/// Threads where I asked something and nobody replied: Follow Up candidates.
pub fn with_open_asks_from_me(store: &Store) -> Result<Vec<ThreadState>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM thread_state")?;
        let all: Vec<ThreadState> = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?.collect::<rusqlite::Result<_>>()?;
        Ok(all.into_iter().filter(|t| !t.open_asks_from_me.is_empty()).collect())
    })
}
