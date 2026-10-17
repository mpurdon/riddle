//! Thread State rows: the JSON of `riddle_core::ThreadState` per thread,
//! with `has_open_asks` beside it so Temporal Sift's Follow Up scan reads
//! the few threads where I asked something instead of deserialising every
//! thread's JSON every fifteen minutes (17 Oct 2026). The column is derived
//! from the state on every `put`; nothing else writes it.

use crate::{from_json, now_ts, to_json, Store};
use riddle_core::{ThreadId, ThreadState};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

pub fn put(store: &Store, s: &ThreadState) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO thread_state (thread_id, json, updated_at, has_open_asks) VALUES (?1, ?2, ?3, ?4)",
            params![
                s.thread_id.as_str(),
                to_json(s)?,
                now_ts(),
                !s.open_asks_from_me.is_empty() as i64
            ],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &ThreadId) -> Result<Option<ThreadState>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM thread_state WHERE thread_id = ?1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

/// The states of several threads in one query (chunked under SQLite's
/// parameter limit), for the per-Find probes of Temporal Sift. Threads
/// without a state are absent.
pub fn get_many(store: &Store, ids: &[ThreadId]) -> Result<HashMap<ThreadId, ThreadState>> {
    let mut out = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(",");
        let found: Vec<ThreadState> = store.read(|c| {
            let mut st = c.prepare(&format!(
                "SELECT json FROM thread_state WHERE thread_id IN ({marks})"
            ))?;
            let rows = st
                .query_map(
                    rusqlite::params_from_iter(chunk.iter().map(|t| t.as_str())),
                    |r| from_json(r.get::<_, String>(0)?),
                )?
                .collect();
            rows
        })?;
        for t in found {
            out.insert(t.thread_id.clone(), t);
        }
    }
    Ok(out)
}

/// Threads where I asked something and nobody replied: Follow Up candidates.
pub fn with_open_asks_from_me(store: &Store) -> Result<Vec<ThreadState>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM thread_state WHERE has_open_asks = 1")?;
        let rows = st
            .query_map([], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreKind;
    use riddle_core::{DigId, Speaker};

    fn state(n: u32, asks: &[&str]) -> ThreadState {
        ThreadState {
            thread_id: ThreadId(format!("t{n}")),
            dig_id: DigId::from("dig-1"),
            last_speaker: Speaker::Me,
            open_asks_from_them: vec![],
            open_asks_from_me: asks.iter().map(|a| a.to_string()).collect(),
            my_commitments: vec![],
            last_reply_by_me_at: None,
            last_message_at: None,
            message_count: 1,
            updated_at: chrono::Utc::now(),
        }
    }

    fn open_asks_column(store: &Store, id: &str) -> i64 {
        store
            .read(|c| {
                c.query_row(
                    "SELECT has_open_asks FROM thread_state WHERE thread_id = ?1",
                    [id],
                    |r| r.get(0),
                )
            })
            .unwrap()
    }

    #[test]
    fn has_open_asks_follows_each_put_and_the_batch_read_matches_the_single_reads() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        put(&store, &state(1, &["Can you send the deck?"])).unwrap();
        put(&store, &state(2, &[])).unwrap();
        put(&store, &state(3, &["Any news?"])).unwrap();
        assert_eq!(open_asks_column(&store, "t1"), 1);
        assert_eq!(open_asks_column(&store, "t2"), 0);
        let mut open: Vec<String> = with_open_asks_from_me(&store)
            .unwrap()
            .into_iter()
            .map(|t| t.thread_id.0)
            .collect();
        open.sort();
        assert_eq!(open, ["t1", "t3"]);

        // Answered: the column follows the state.
        put(&store, &state(1, &[])).unwrap();
        assert_eq!(open_asks_column(&store, "t1"), 0);
        assert_eq!(with_open_asks_from_me(&store).unwrap().len(), 1);

        let ids = [
            ThreadId("t1".into()),
            ThreadId("t3".into()),
            ThreadId("missing".into()),
        ];
        let batch = get_many(&store, &ids).unwrap();
        assert_eq!(batch.len(), 2);
        for id in &ids {
            assert_eq!(batch.get(id), get(&store, id).unwrap().as_ref());
        }
    }
}
