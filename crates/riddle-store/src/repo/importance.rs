//! The user's own Importance for a message. Kept apart from the
//! classification so a rule re-deciding the message never overwrites it;
//! lists read it first and fall back to the decision's level.

use crate::{now_ts, Store};
use riddle_core::{Importance, MessageId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn get(store: &Store, id: &MessageId) -> Result<Option<Importance>> {
    store.read(|c| {
        c.query_row(
            "SELECT importance FROM importance_overrides WHERE message_id = ?1",
            [id.as_str()],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map(|v| v.map(Importance::from_rank))
    })
}

/// Set (`Some`) or clear (`None`) the user's level for one message.
pub fn set(store: &Store, id: &MessageId, level: Option<Importance>) -> Result<()> {
    store.write(|c| {
        match level {
            Some(l) => c.execute(
                "INSERT INTO importance_overrides (message_id, importance, set_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(message_id) DO UPDATE SET importance = excluded.importance, set_at = excluded.set_at",
                params![id.as_str(), l.rank(), now_ts()],
            ),
            None => c.execute(
                "DELETE FROM importance_overrides WHERE message_id = ?1",
                [id.as_str()],
            ),
        }
        .map(|_| ())
    })
}
