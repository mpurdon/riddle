use crate::{from_json, now_ts, to_json, Store};
use riddle_core::{Person, PersonId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, p: &Person) -> Result<()> {
    store.write(|c| {
        c.execute("INSERT INTO people (id, json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(id) DO UPDATE SET json = excluded.json, updated_at = excluded.updated_at", params![p.id.as_str(), to_json(p)?, now_ts()])?;
        for e in &p.emails {
            c.execute("INSERT OR REPLACE INTO person_emails (email, person_id) VALUES (?1, ?2)", params![e.to_lowercase(), p.id.as_str()])?;
        }
        Ok(())
    })
}

pub fn get(store: &Store, id: &PersonId) -> Result<Option<Person>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM people WHERE id = ?1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

pub fn by_email(store: &Store, email: &str) -> Result<Option<Person>> {
    store.read(|c| {
        c.query_row(
            "SELECT p.json FROM people p JOIN person_emails e ON e.person_id = p.id WHERE e.email = ?1",
            [email.to_lowercase()], |r| from_json(r.get::<_, String>(0)?),
        ).optional()
    })
}

pub fn list(store: &Store, limit: u32) -> Result<Vec<Person>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM people ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = st
            .query_map([limit], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn search(store: &Store, q: &str, limit: u32) -> Result<Vec<Person>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT DISTINCT p.json FROM people p JOIN person_emails e ON e.person_id = p.id WHERE e.email LIKE ?1 OR p.json LIKE ?1 LIMIT ?2")?;
        let rows = st.query_map(params![format!("%{}%", q.to_lowercase()), limit], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// Whether the user knows anyone at this domain: a contact, or someone
/// they replied to. For `Facts::known_domain`.
pub fn known_at_domain(store: &Store, domain: &str) -> Result<bool> {
    let people: Vec<Person> = store.read(|c| {
        let mut st = c.prepare(
            "SELECT DISTINCT p.json FROM people p JOIN person_emails e ON e.person_id = p.id WHERE e.email LIKE ?1",
        )?;
        let rows = st
            .query_map([format!("%@{}", domain.to_lowercase())], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })?;
    Ok(people.iter().any(|p| p.replied_to_by_user || p.in_contacts))
}
