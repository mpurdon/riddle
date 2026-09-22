use crate::{from_json, to_json, ts, Store};
use riddle_core::{Find, FindId, FindStatus, FindType, MessageId, Page, PageRequest};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, f: &Find) -> Result<()> {
    store.write(|c| upsert_in(c, f))
}

pub fn upsert_in(c: &rusqlite::Connection, f: &Find) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO finds (id, find_type, status, priority, identity_key, due_at, snoozed_until, json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET find_type = excluded.find_type, status = excluded.status, priority = excluded.priority,
           identity_key = excluded.identity_key, due_at = excluded.due_at, snoozed_until = excluded.snoozed_until,
           json = excluded.json, updated_at = excluded.updated_at",
        params![
            f.id.as_str(), f.find_type.as_str(), f.status.as_str(), f.priority as i64, f.identity_key,
            f.due_at.as_ref().map(ts), f.snoozed_until.as_ref().map(ts), to_json(f)?, ts(&f.created_at), ts(&f.updated_at)
        ],
    )?;
    c.execute("DELETE FROM find_sources WHERE find_id = ?1", [f.id.as_str()])?;
    for s in &f.sources {
        c.execute("INSERT OR IGNORE INTO find_sources (find_id, message_id, dig_id) VALUES (?1, ?2, ?3)", params![f.id.as_str(), s.message_id.as_str(), s.dig_id.as_str()])?;
    }
    Ok(())
}

pub fn get(store: &Store, id: &FindId) -> Result<Option<Find>> {
    store.read(|c| c.query_row("SELECT json FROM finds WHERE id = ?1", [id.as_str()], |r| from_json(r.get::<_, String>(0)?)).optional())
}

pub fn by_identity(store: &Store, key: &str) -> Result<Option<Find>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM finds WHERE identity_key = ?1 AND status IN ('open','snoozed') ORDER BY created_at DESC LIMIT 1",
            [key], |r| from_json(r.get::<_, String>(0)?),
        ).optional()
    })
}

pub fn for_message(store: &Store, message: &MessageId) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT f.json FROM finds f JOIN find_sources s ON s.find_id = f.id WHERE s.message_id = ?1 ORDER BY f.created_at DESC")?;
        let rows = st.query_map([message.as_str()], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

#[derive(Debug, Default, Clone)]
pub struct FindFilter {
    pub status: Option<FindStatus>,
    pub find_type: Option<FindType>,
    pub dig_id: Option<String>,
    pub min_priority: Option<u8>,
}

pub fn list(store: &Store, filter: &FindFilter, page: &PageRequest) -> Result<Page<Find>> {
    let limit = page.limit();
    let offset = page.offset();
    store.read(|c| {
        let mut sql = String::from("SELECT DISTINCT f.json, f.priority, f.created_at FROM finds f");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if filter.dig_id.is_some() { sql.push_str(" JOIN find_sources s ON s.find_id = f.id"); }
        sql.push_str(" WHERE 1=1");
        if let Some(s) = filter.status { sql.push_str(" AND f.status = ?"); args.push(Box::new(s.as_str())); }
        if let Some(t) = filter.find_type { sql.push_str(" AND f.find_type = ?"); args.push(Box::new(t.as_str())); }
        if let Some(d) = &filter.dig_id { sql.push_str(" AND s.dig_id = ?"); args.push(Box::new(d.clone())); }
        if let Some(p) = filter.min_priority { sql.push_str(" AND f.priority >= ?"); args.push(Box::new(p as i64)); }
        let count_sql = format!("SELECT count(*) FROM ({sql})");
        let total: i64 = c.query_row(&count_sql, rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| r.get(0))?;
        sql.push_str(" ORDER BY f.priority DESC, f.created_at DESC LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));
        let mut st = c.prepare(&sql)?;
        let items = st.query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| from_json(r.get::<_, String>(0)?))?.collect::<rusqlite::Result<Vec<Find>>>()?;
        Ok(Page::from_offset(items, offset, limit, Some(total as u64)))
    })
}

pub fn count_open(store: &Store) -> Result<u64> {
    store.read(|c| c.query_row("SELECT count(*) FROM finds WHERE status = 'open'", [], |r| r.get::<_, i64>(0)).map(|n| n as u64))
}

/// Snoozed Finds whose snooze has ended, and dated Finds past their date.
pub fn due_for_temporal(store: &Store, now: &chrono::DateTime<chrono::Utc>) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM finds WHERE (status = 'snoozed' AND snoozed_until <= ?1) OR (status = 'open' AND due_at IS NOT NULL AND due_at < ?1)",
        )?;
        let rows = st.query_map([ts(now)], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

pub fn open_by_thread(store: &Store, thread_provider_id: &str) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM finds WHERE status IN ('open','snoozed') AND identity_key = ?1")?;
        let rows = st.query_map([thread_provider_id], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// Every Find in a status, optionally of one type; Temporal Sift walks these.
pub fn list_by_status(store: &Store, status: FindStatus, find_type: Option<FindType>) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM finds WHERE status = ?1 AND (?2 IS NULL OR find_type = ?2) ORDER BY created_at")?;
        let rows = st.query_map(params![status.as_str(), find_type.map(|t| t.as_str())], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// Any Find (open or not) with an identity key, newest first; dedupe checks
/// that must not recreate a dismissed Follow Up read this.
pub fn any_by_identity(store: &Store, key: &str) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM finds WHERE identity_key = ?1 ORDER BY created_at DESC")?;
        let rows = st.query_map([key], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}
