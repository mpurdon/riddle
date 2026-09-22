use crate::{from_json, from_ts, to_json, ts, Store};
use riddle_core::{Activity, ActivityId, Actor, Page, PageRequest};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn append(store: &Store, a: &Activity) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO activities (id, actor, action, dig_id, json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![a.id.as_str(), a.actor.as_str(), a.action, a.dig_id.as_ref().map(|d| d.as_str()), to_json(a)?, ts(&a.created_at)],
        )?;
        Ok(())
    })
}

pub fn update(store: &Store, a: &Activity) -> Result<()> {
    store.write(|c| {
        c.execute("UPDATE activities SET json = ?2 WHERE id = ?1", params![a.id.as_str(), to_json(a)?])?;
        Ok(())
    })
}

pub fn get(store: &Store, id: &ActivityId) -> Result<Option<Activity>> {
    store.read(|c| c.query_row("SELECT json FROM activities WHERE id = ?1", [id.as_str()], |r| from_json(r.get::<_, String>(0)?)).optional())
}

#[derive(Debug, Default, Clone)]
pub struct ActivityFilter {
    pub dig_id: Option<String>,
    pub actor: Option<Actor>,
    pub action: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn list(store: &Store, filter: &ActivityFilter, page: &PageRequest) -> Result<Page<Activity>> {
    let limit = page.limit();
    let offset = page.offset();
    store.read(|c| {
        let mut sql = String::from("SELECT json FROM activities WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(d) = &filter.dig_id { sql.push_str(" AND dig_id = ?"); args.push(Box::new(d.clone())); }
        if let Some(a) = filter.actor { sql.push_str(" AND actor = ?"); args.push(Box::new(a.as_str())); }
        if let Some(a) = &filter.action { sql.push_str(" AND action = ?"); args.push(Box::new(a.clone())); }
        if let Some(s) = &filter.since { sql.push_str(" AND created_at >= ?"); args.push(Box::new(ts(s))); }
        if let Some(u) = &filter.until { sql.push_str(" AND created_at <= ?"); args.push(Box::new(ts(u))); }
        let count_sql = sql.replace("SELECT json", "SELECT count(*)");
        let total: i64 = c.query_row(&count_sql, rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| r.get(0))?;
        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));
        let mut st = c.prepare(&sql)?;
        let items = st.query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| from_json(r.get::<_, String>(0)?))?.collect::<rusqlite::Result<Vec<Activity>>>()?;
        Ok(Page::from_offset(items, offset, limit, Some(total as u64)))
    })
}

/// Activities after a point in time, oldest first; used by undo to find later
/// touches on the same entities.
pub fn since(store: &Store, after: &chrono::DateTime<chrono::Utc>) -> Result<Vec<Activity>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM activities WHERE created_at > ?1 ORDER BY created_at")?;
        let rows = st.query_map([ts(after)], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

pub fn latest_at(store: &Store) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    store.read(|c| Ok(c.query_row("SELECT MAX(created_at) FROM activities", [], |r| r.get::<_, Option<i64>>(0))?.map(from_ts)))
}
