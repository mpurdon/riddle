//! The user's Agree/Disagree on Shadow proposals, one row per message in the
//! Dig store, so the message list can join it and a row can say "Agreed"
//! without a second query per account. The marks first lived in the index
//! store's kv table, which the Dig store's list could not reach.

use crate::{ts, Store};
use riddle_core::{MessageId, ShadowReview};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn set(store: &Store, id: &MessageId, review: ShadowReview) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT OR REPLACE INTO shadow_reviews (message_id, review, created_at) VALUES (?1, ?2, ?3)",
            params![id.as_str(), review.as_str(), ts(&chrono::Utc::now())],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &MessageId) -> Result<Option<ShadowReview>> {
    store.read(|c| {
        Ok(c.query_row(
            "SELECT review FROM shadow_reviews WHERE message_id = ?1",
            [id.as_str()],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|s| ShadowReview::parse(&s)))
    })
}

/// Every review on this Dig, message id → review.
pub fn all(store: &Store) -> Result<Vec<(MessageId, ShadowReview)>> {
    store.read(|c| {
        let mut st =
            c.prepare("SELECT message_id, review FROM shadow_reviews ORDER BY created_at")?;
        let rows = st
            .query_map([], |r| Ok((MessageId(r.get(0)?), r.get::<_, String>(1)?)))?
            .filter_map(|row| match row {
                Ok((id, s)) => ShadowReview::parse(&s).map(|rv| Ok((id, rv))),
                Err(e) => Some(Err(e)),
            })
            .collect();
        rows
    })
}
