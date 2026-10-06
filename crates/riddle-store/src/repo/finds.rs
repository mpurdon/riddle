use crate::{from_json, to_json, ts, Store};
use riddle_core::{DigId, Find, FindId, FindStatus, FindType, MessageId, Page, PageRequest};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, f: &Find) -> Result<()> {
    store.write(|c| upsert_in(c, f))
}

pub fn upsert_in(c: &rusqlite::Connection, f: &Find) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO finds (id, find_type, status, priority, identity_key, due_at, snoozed_until, json, created_at, updated_at, series_key, expires_at, kept_open_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET find_type = excluded.find_type, status = excluded.status, priority = excluded.priority,
           identity_key = excluded.identity_key, due_at = excluded.due_at, snoozed_until = excluded.snoozed_until,
           json = excluded.json, updated_at = excluded.updated_at, series_key = excluded.series_key, expires_at = excluded.expires_at, kept_open_at = excluded.kept_open_at",
        params![
            f.id.as_str(), f.find_type.as_str(), f.status.as_str(), f.priority as i64, f.identity_key,
            f.due_at.as_ref().map(ts), f.snoozed_until.as_ref().map(ts), to_json(f)?, ts(&f.created_at), ts(&f.updated_at),
            f.series.as_ref().map(|s| s.key.as_str()), f.expires_at.as_ref().map(ts), f.kept_open_at.as_ref().map(ts)
        ],
    )?;
    c.execute(
        "DELETE FROM find_sources WHERE find_id = ?1",
        [f.id.as_str()],
    )?;
    for s in &f.sources {
        c.execute(
            "INSERT OR IGNORE INTO find_sources (find_id, message_id, dig_id) VALUES (?1, ?2, ?3)",
            params![f.id.as_str(), s.message_id.as_str(), s.dig_id.as_str()],
        )?;
    }
    Ok(())
}

pub fn get(store: &Store, id: &FindId) -> Result<Option<Find>> {
    store.read(|c| {
        c.query_row("SELECT json FROM finds WHERE id = ?1", [id.as_str()], |r| {
            from_json(r.get::<_, String>(0)?)
        })
        .optional()
    })
}

pub fn by_identity(store: &Store, key: &str) -> Result<Option<Find>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM finds WHERE identity_key = ?1 AND status IN ('open','snoozed') ORDER BY created_at DESC LIMIT 1",
            [key], |r| from_json(r.get::<_, String>(0)?),
        ).optional()
    })
}

/// The open and snoozed Finds of a `+ Replaces` series, leaving out any the
/// user kept: no rule closes those.
pub fn open_in_series(store: &Store, key: &str) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM finds WHERE series_key = ?1 AND status IN ('open','snoozed') AND kept_open_at IS NULL",
        )?;
        let rows = st
            .query_map([key], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

pub fn for_message(store: &Store, message: &MessageId) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT f.json FROM finds f JOIN find_sources s ON s.find_id = f.id WHERE s.message_id = ?1 ORDER BY f.created_at DESC")?;
        let rows = st.query_map([message.as_str()], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// A Find belongs to the work unless every account it came from is disabled
/// (`Dig::disabled`). Finds span accounts here in the index, so lists and
/// counts filter in SQL instead of trusting each caller's account list. A
/// source whose account row is gone counts as enabled, as before.
const ON_AN_ENABLED_DIG: &str = "(NOT EXISTS (SELECT 1 FROM find_sources x WHERE x.find_id = f.id)
    OR EXISTS (SELECT 1 FROM find_sources x LEFT JOIN digs d ON d.id = x.dig_id
               WHERE x.find_id = f.id AND coalesce(json_extract(d.json, '$.disabled'), 0) = 0))";

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
        if filter.dig_id.is_some() {
            sql.push_str(" JOIN find_sources s ON s.find_id = f.id");
        }
        sql.push_str(" WHERE ");
        sql.push_str(ON_AN_ENABLED_DIG);
        if let Some(s) = filter.status {
            sql.push_str(" AND f.status = ?");
            args.push(Box::new(s.as_str()));
        }
        if let Some(t) = filter.find_type {
            sql.push_str(" AND f.find_type = ?");
            args.push(Box::new(t.as_str()));
        }
        if let Some(d) = &filter.dig_id {
            sql.push_str(" AND s.dig_id = ?");
            args.push(Box::new(d.clone()));
        }
        if let Some(p) = filter.min_priority {
            sql.push_str(" AND f.priority >= ?");
            args.push(Box::new(p as i64));
        }
        let count_sql = format!("SELECT count(*) FROM ({sql})");
        let total: i64 = c.query_row(
            &count_sql,
            rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())),
            |r| r.get(0),
        )?;
        sql.push_str(" ORDER BY f.priority DESC, f.created_at DESC LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));
        let mut st = c.prepare(&sql)?;
        let items = st
            .query_map(
                rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())),
                |r| from_json(r.get::<_, String>(0)?),
            )?
            .collect::<rusqlite::Result<Vec<Find>>>()?;
        Ok(Page::from_offset(items, offset, limit, Some(total as u64)))
    })
}

/// The (Dig, message) sources of every open Find at or above `min_priority`,
/// of the given types (any type when empty), for the Priority inbox. Finds
/// live here in the index; the Dig store's message query takes the ids.
pub fn open_source_ids(
    store: &Store,
    min_priority: u8,
    find_types: &[FindType],
) -> Result<Vec<(DigId, MessageId)>> {
    store.read(|c| {
        let mut sql = String::from(
            "SELECT DISTINCT s.dig_id, s.message_id FROM finds f JOIN find_sources s ON s.find_id = f.id
             LEFT JOIN digs d ON d.id = s.dig_id
             WHERE f.status = 'open' AND f.priority >= ?
               AND coalesce(json_extract(d.json, '$.disabled'), 0) = 0",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(min_priority as i64)];
        if !find_types.is_empty() {
            let marks = vec!["?"; find_types.len()].join(",");
            sql.push_str(&format!(" AND f.find_type IN ({marks})"));
            for t in find_types {
                args.push(Box::new(t.as_str()));
            }
        }
        let mut st = c.prepare(&sql)?;
        let rows = st
            .query_map(
                rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())),
                |r| {
                    Ok((
                        DigId::from(r.get::<_, String>(0)?.as_str()),
                        MessageId(r.get(1)?),
                    ))
                },
            )?
            .collect();
        rows
    })
}

/// Open Finds, everywhere or with a source in one Dig. A count, not a page
/// of one: the summary asks per Dig on every refresh.
pub fn count_open(store: &Store, dig_id: Option<&DigId>) -> Result<u64> {
    store.read(|c| {
        match dig_id {
            None => c.query_row(
                &format!("SELECT count(*) FROM finds f WHERE f.status = 'open' AND {ON_AN_ENABLED_DIG}"),
                [],
                |r| r.get::<_, i64>(0),
            ),
            Some(d) => c.query_row(
                "SELECT count(*) FROM finds f WHERE f.status = 'open'
                   AND EXISTS (SELECT 1 FROM find_sources s WHERE s.find_id = f.id AND s.dig_id = ?1)",
                [d.as_str()],
                |r| r.get::<_, i64>(0),
            ),
        }
        .map(|n| n as u64)
    })
}

/// Snoozed Finds whose snooze has ended, and, unless the user kept them,
/// dated Finds past their date and Finds past their `+ Expires:` time. A
/// disabled account's Finds wait until it is enabled again.
pub fn due_for_temporal(store: &Store, now: &chrono::DateTime<chrono::Utc>) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT f.json FROM finds f WHERE ((f.status = 'snoozed' AND f.snoozed_until <= ?1)
               OR (f.kept_open_at IS NULL AND ((f.status = 'open' AND f.due_at IS NOT NULL AND f.due_at < ?1)
                 OR (f.status IN ('open','snoozed') AND f.expires_at IS NOT NULL AND f.expires_at <= ?1))))
               AND {ON_AN_ENABLED_DIG}"
        ))?;
        let rows = st.query_map([ts(now)], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

pub fn open_by_thread(store: &Store, thread_provider_id: &str) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM finds WHERE status IN ('open','snoozed') AND identity_key = ?1",
        )?;
        let rows = st
            .query_map([thread_provider_id], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// Every Find in a status, optionally of one type, on an enabled account;
/// Temporal Sift walks these.
pub fn list_by_status(
    store: &Store,
    status: FindStatus,
    find_type: Option<FindType>,
) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st = c.prepare(&format!("SELECT f.json FROM finds f WHERE f.status = ?1 AND (?2 IS NULL OR f.find_type = ?2) AND {ON_AN_ENABLED_DIG} ORDER BY f.created_at"))?;
        let rows = st.query_map(params![status.as_str(), find_type.map(|t| t.as_str())], |r| from_json(r.get::<_, String>(0)?))?.collect();
        rows
    })
}

/// Any Find (open or not) with an identity key, newest first; dedupe checks
/// that must not recreate a dismissed Follow Up read this.
pub fn any_by_identity(store: &Store, key: &str) -> Result<Vec<Find>> {
    store.read(|c| {
        let mut st =
            c.prepare("SELECT json FROM finds WHERE identity_key = ?1 ORDER BY created_at DESC")?;
        let rows = st
            .query_map([key], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreKind;
    use riddle_core::{FindSource, ThreadId};

    fn find(n: u32, find_type: FindType, priority: u8, status: FindStatus) -> Find {
        let dig = DigId::from("dig-1");
        let now = chrono::Utc::now();
        Find {
            code: None,
            id: FindId::new(),
            find_type,
            title: format!("Find {n}"),
            summary: String::new(),
            priority,
            status,
            due_at: None,
            snoozed_until: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
            resolved_by: None,
            sources: vec![FindSource {
                message_id: MessageId::for_provider(&dig, &format!("m{n}")),
                thread_id: ThreadId::for_provider(&dig, &format!("t{n}")),
                dig_id: dig.clone(),
            }],
            dig_ids: vec![dig],
            identity_key: format!("key-{n}"),
            reason: String::new(),
            confidence: None,
            classification_id: None,
            trace_id: None,
            superseded_by: None,
            series: None,
            expires_at: None,
            kept_open_at: None,
            notify: false,
        }
    }

    #[test]
    fn open_source_ids_honours_the_priority_floor_the_types_and_the_status() {
        let store = Store::in_memory(StoreKind::Index).unwrap();
        let high_pay = find(1, FindType::Pay, 90, FindStatus::Open);
        let high_review = find(2, FindType::Review, 85, FindStatus::Open);
        let low = find(3, FindType::Pay, 40, FindStatus::Open);
        let resolved = find(4, FindType::Pay, 95, FindStatus::Resolved);
        for f in [&high_pay, &high_review, &low, &resolved] {
            upsert(&store, f).unwrap();
        }
        let dig = DigId::from("dig-1");
        let mid = |f: &Find| f.sources[0].message_id.clone();
        let mut any = open_source_ids(&store, 70, &[]).unwrap();
        any.sort();
        let mut want = vec![
            (dig.clone(), mid(&high_pay)),
            (dig.clone(), mid(&high_review)),
        ];
        want.sort();
        assert_eq!(any, want);
        assert_eq!(
            open_source_ids(&store, 70, &[FindType::Pay]).unwrap(),
            vec![(dig.clone(), mid(&high_pay))]
        );
        assert_eq!(
            open_source_ids(&store, 0, &[]).unwrap().len(),
            3,
            "any priority, still only open"
        );
    }
}
