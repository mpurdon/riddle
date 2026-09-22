//! Director conversations (§43). The transcript is kept for context and the
//! Director Inspector; outcomes live elsewhere as Meshes, Finds, Activity.

use crate::{from_json, to_json, ts, Store};
use riddle_core::{Conversation, ConversationId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, c: &Conversation) -> Result<()> {
    store.write(|conn| {
        conn.execute(
            "INSERT INTO conversations (id, json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json, updated_at = excluded.updated_at",
            params![c.id.as_str(), to_json(c)?, ts(&c.updated_at)],
        )?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &ConversationId) -> Result<Option<Conversation>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM conversations WHERE id = ?1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

/// Newest first, without turns (the list view never needs them).
pub fn list(store: &Store, limit: u32) -> Result<Vec<Conversation>> {
    let full: Vec<Conversation> = store.read(|c| {
        let mut st =
            c.prepare("SELECT json FROM conversations ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = st
            .query_map([limit], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })?;
    Ok(full
        .into_iter()
        .map(|mut c| {
            c.turns.clear();
            c
        })
        .collect())
}

pub fn remove(store: &Store, id: &ConversationId) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM conversations WHERE id = ?1", [id.as_str()])?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreKind;
    use riddle_core::{Turn, TurnRole};

    #[test]
    fn a_conversation_round_trips_and_the_list_omits_turns() {
        let store = Store::in_memory(StoreKind::Index).unwrap();
        let now = chrono::Utc::now();
        let c = Conversation {
            id: ConversationId::new(),
            title: "hello".into(),
            turns: vec![Turn {
                role: TurnRole::User,
                content: "hi".into(),
                tool_calls: vec![],
                created_at: now,
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: None,
            }],
            read_untrusted: false,
            dig_ids: vec![],
            model_id: None,
            prompt_version: None,
            created_at: now,
            updated_at: now,
        };
        upsert(&store, &c).unwrap();
        assert_eq!(get(&store, &c.id).unwrap().unwrap().turns.len(), 1);
        let listed = list(&store, 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].turns.is_empty());
        remove(&store, &c.id).unwrap();
        assert!(get(&store, &c.id).unwrap().is_none());
    }
}
