//! Messages and threads in a Dig store. Bodies are nullable and fetched
//! lazily; list rows never load them.

use crate::{from_json, from_ts, now_ts, to_json, ts, Store};
use riddle_core::{Address, AttachmentMeta, DigId, Message, MessageHeaders, MessageId, MessageSummary, Page, PageRequest, ThreadId};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension, Row};

fn row_to_message(r: &Row<'_>, dig_id: &DigId) -> rusqlite::Result<Message> {
    Ok(Message {
        id: MessageId(r.get("id")?),
        dig_id: dig_id.clone(),
        thread_id: ThreadId(r.get("thread_id")?),
        provider_id: r.get("provider_id")?,
        subject: r.get("subject")?,
        from: Address { email: r.get("from_email")?, name: r.get("from_name")? },
        to: from_json(r.get("to_json")?)?,
        cc: from_json(r.get("cc_json")?)?,
        bcc: from_json(r.get("bcc_json")?)?,
        date: from_ts(r.get("date")?),
        snippet: r.get("snippet")?,
        labels: from_json(r.get("labels_json")?)?,
        headers: from_json::<MessageHeaders>(r.get("headers_json")?)?,
        attachments: from_json::<Vec<AttachmentMeta>>(r.get("attachments_json")?)?,
        size_bytes: r.get::<_, i64>("size_bytes")? as u64,
        is_sent: r.get::<_, i64>("is_sent")? != 0,
        has_body: r.get::<_, Option<String>>("body_text")?.is_some() || r.get::<_, Option<String>>("body_html")?.is_some(),
        body_text: r.get("body_text")?,
        body_html: r.get("body_html")?,
        synced_at: from_ts(r.get("synced_at")?),
    })
}

const COLS: &str = "id, provider_id, thread_id, subject, from_email, from_name, to_json, cc_json, bcc_json, date, snippet, labels_json, headers_json, attachments_json, size_bytes, is_sent, body_text, body_html, synced_at";

/// Insert or update. Bodies already stored are kept when the incoming
/// message has none (a metadata-only resync must not drop fetched bodies).
pub fn upsert(store: &Store, m: &Message) -> Result<()> {
    store.write(|c| upsert_in(c, m))
}

pub fn upsert_in(c: &rusqlite::Connection, m: &Message) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO messages (id, provider_id, thread_id, subject, from_email, from_name, to_json, cc_json, bcc_json, date, snippet, labels_json, headers_json, attachments_json, size_bytes, is_sent, body_text, body_html, synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
         ON CONFLICT(id) DO UPDATE SET
           subject = excluded.subject, from_email = excluded.from_email, from_name = excluded.from_name,
           to_json = excluded.to_json, cc_json = excluded.cc_json, bcc_json = excluded.bcc_json, date = excluded.date,
           snippet = excluded.snippet, labels_json = excluded.labels_json, headers_json = excluded.headers_json,
           attachments_json = excluded.attachments_json, size_bytes = excluded.size_bytes, is_sent = excluded.is_sent,
           body_text = COALESCE(excluded.body_text, messages.body_text),
           body_html = COALESCE(excluded.body_html, messages.body_html),
           synced_at = excluded.synced_at, deleted_at = NULL",
        params![
            m.id.as_str(), m.provider_id, m.thread_id.as_str(), m.subject, m.from.email, m.from.name,
            to_json(&m.to)?, to_json(&m.cc)?, to_json(&m.bcc)?, ts(&m.date), m.snippet, to_json(&m.labels)?,
            to_json(&m.headers)?, to_json(&m.attachments)?, m.size_bytes as i64, m.is_sent as i64,
            m.body_text, m.body_html, ts(&m.synced_at)
        ],
    )?;
    c.execute(
        "INSERT INTO threads (id, provider_id, subject, last_message_at, message_count) VALUES (?1, ?2, ?3, ?4, 1)
         ON CONFLICT(id) DO UPDATE SET last_message_at = MAX(threads.last_message_at, excluded.last_message_at),
           message_count = (SELECT count(*) FROM messages WHERE thread_id = ?1 AND deleted_at IS NULL)",
        params![m.thread_id.as_str(), m.thread_id.provider_id(), m.subject, ts(&m.date)],
    )?;
    Ok(())
}

pub fn get(store: &Store, dig_id: &DigId, id: &MessageId) -> Result<Option<Message>> {
    store.read(|c| {
        c.query_row(&format!("SELECT {COLS} FROM messages WHERE id = ?1 AND deleted_at IS NULL"), [id.as_str()], |r| row_to_message(r, dig_id)).optional()
    })
}

pub fn get_many(store: &Store, dig_id: &DigId, ids: &[MessageId]) -> Result<Vec<Message>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(m) = get(store, dig_id, id)? { out.push(m); }
    }
    Ok(out)
}

pub fn set_body(store: &Store, id: &MessageId, text: Option<&str>, html: Option<&str>) -> Result<()> {
    store.write(|c| {
        c.execute("UPDATE messages SET body_text = ?2, body_html = ?3 WHERE id = ?1", params![id.as_str(), text, html])?;
        Ok(())
    })
}

pub fn set_labels(store: &Store, id: &MessageId, labels: &[String]) -> Result<()> {
    store.write(|c| {
        c.execute("UPDATE messages SET labels_json = ?2 WHERE id = ?1", params![id.as_str(), to_json(&labels)?])?;
        Ok(())
    })
}

pub fn mark_deleted(store: &Store, id: &MessageId) -> Result<()> {
    store.write(|c| { c.execute("UPDATE messages SET deleted_at = ?2 WHERE id = ?1", params![id.as_str(), now_ts()])?; Ok(()) })
}

pub fn labels_of(store: &Store, id: &MessageId) -> Result<Vec<String>> {
    store.read(|c| {
        Ok(c.query_row("SELECT labels_json FROM messages WHERE id = ?1", [id.as_str()], |r| from_json::<Vec<String>>(r.get(0)?)).optional()?.unwrap_or_default())
    })
}

pub fn thread_messages(store: &Store, dig_id: &DigId, thread: &ThreadId) -> Result<Vec<Message>> {
    store.read(|c| {
        let mut st = c.prepare(&format!("SELECT {COLS} FROM messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY date"))?;
        let rows = st.query_map([thread.as_str()], |r| row_to_message(r, dig_id))?.collect();
        rows
    })
}

pub fn count(store: &Store) -> Result<u64> {
    store.read(|c| c.query_row("SELECT count(*) FROM messages WHERE deleted_at IS NULL", [], |r| r.get::<_, i64>(0)).map(|n| n as u64))
}

pub fn count_since(store: &Store, since: &chrono::DateTime<chrono::Utc>) -> Result<u64> {
    store.read(|c| c.query_row("SELECT count(*) FROM messages WHERE deleted_at IS NULL AND date >= ?1", [ts(since)], |r| r.get::<_, i64>(0)).map(|n| n as u64))
}

/// Message ids with no current classification, newest first: the Sift backlog.
pub fn unclassified_ids(store: &Store, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT m.id FROM messages m
             WHERE m.deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM classifications k WHERE k.message_id = m.id AND k.superseded_at IS NULL)
             ORDER BY m.date DESC LIMIT ?1",
        )?;
        let rows = st.query_map([limit], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}

pub fn unclassified_count(store: &Store) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM messages m WHERE m.deleted_at IS NULL AND NOT EXISTS (SELECT 1 FROM classifications k WHERE k.message_id = m.id AND k.superseded_at IS NULL)",
            [], |r| r.get::<_, i64>(0),
        ).map(|n| n as u64)
    })
}

/// Filters for the paged list. `disposition` matches the current
/// classification; `label` matches a provider label; `query` is FTS5.
#[derive(Debug, Default, Clone)]
pub struct MessageFilter {
    pub disposition: Option<String>,
    pub find_type: Option<String>,
    pub label: Option<String>,
    pub query: Option<String>,
    pub from: Option<String>,
    pub thread_id: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub unclassified_only: bool,
    pub include_shadow: bool,
    pub stratum: Option<String>,
}

pub fn list(store: &Store, dig_id: &DigId, filter: &MessageFilter, page: &PageRequest) -> Result<Page<MessageSummary>> {
    let limit = page.limit();
    let offset = page.offset();
    store.read(|c| {
        let mut sql = String::from(
            "SELECT m.id, m.thread_id, m.subject, m.from_email, m.from_name, m.date, m.snippet, m.labels_json, m.is_sent, m.attachments_json,
                    k.json AS class_json
             FROM messages m
             LEFT JOIN classifications k ON k.message_id = m.id AND k.superseded_at IS NULL AND k.shadow = 0
             WHERE m.deleted_at IS NULL",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(d) = &filter.disposition {
            sql.push_str(" AND k.disposition_kind = ?");
            args.push(Box::new(d.clone()));
        }
        if let Some(f) = &filter.find_type {
            sql.push_str(" AND k.find_type = ?");
            args.push(Box::new(f.clone()));
        }
        if filter.unclassified_only {
            sql.push_str(" AND k.id IS NULL");
        }
        if let Some(l) = &filter.label {
            sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(m.labels_json) WHERE value = ?)");
            args.push(Box::new(l.clone()));
        }
        if let Some(s) = &filter.stratum {
            sql.push_str(" AND EXISTS (SELECT 1 FROM message_strata ms WHERE ms.message_id = m.id AND ms.stratum = ?)");
            args.push(Box::new(s.clone()));
        }
        if let Some(f) = &filter.from {
            sql.push_str(" AND m.from_email LIKE ?");
            args.push(Box::new(format!("%{}%", f.to_lowercase())));
        }
        if let Some(t) = &filter.thread_id {
            sql.push_str(" AND m.thread_id = ?");
            args.push(Box::new(t.clone()));
        }
        if let Some(s) = &filter.since { sql.push_str(" AND m.date >= ?"); args.push(Box::new(ts(s))); }
        if let Some(u) = &filter.until { sql.push_str(" AND m.date <= ?"); args.push(Box::new(ts(u))); }
        if let Some(q) = filter.query.as_ref().filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND m.rowid IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)");
            args.push(Box::new(fts_query(q)));
        }
        let count_sql = format!("SELECT count(*) FROM ({sql})");
        let total: i64 = c.query_row(&count_sql, rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| r.get(0))?;
        sql.push_str(" ORDER BY m.date DESC LIMIT ? OFFSET ?");
        args.push(Box::new(limit));
        args.push(Box::new(offset));
        let mut st = c.prepare(&sql)?;
        let items = st
            .query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| {
                let class: Option<riddle_core::Classification> = r.get::<_, Option<String>>("class_json")?.map(from_json).transpose()?;
                let attachments: Vec<AttachmentMeta> = from_json(r.get("attachments_json")?)?;
                Ok(MessageSummary {
                    id: MessageId(r.get("id")?),
                    dig_id: dig_id.clone(),
                    thread_id: ThreadId(r.get("thread_id")?),
                    subject: r.get("subject")?,
                    from: Address { email: r.get("from_email")?, name: r.get("from_name")? },
                    date: from_ts(r.get("date")?),
                    snippet: r.get("snippet")?,
                    labels: from_json(r.get("labels_json")?)?,
                    is_sent: r.get::<_, i64>("is_sent")? != 0,
                    has_attachments: !attachments.is_empty(),
                    disposition: class.as_ref().map(|k| k.disposition.clone()),
                    strata: class.as_ref().map(|k| k.strata.clone()).unwrap_or_default(),
                    find_id: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Page::from_offset(items, offset, limit, Some(total as u64)))
    })
}

/// Quote each term so user input cannot break the FTS5 grammar; prefix match
/// on the last term so typing feels live.
pub fn fts_query(q: &str) -> String {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .collect();
    let n = terms.len();
    terms
        .into_iter()
        .enumerate()
        .map(|(i, t)| if i + 1 == n { format!("\"{t}\"*") } else { format!("\"{t}\"") })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Distinct sender addresses the user has replied to, for facts.
pub fn replied_to_senders(store: &Store) -> Result<std::collections::HashSet<String>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT DISTINCT to_json FROM messages WHERE is_sent = 1 AND deleted_at IS NULL")?;
        let rows = st.query_map([], |r| from_json::<Vec<Address>>(r.get(0)?))?;
        let mut set = std::collections::HashSet::new();
        for row in rows {
            for a in row? { set.insert(a.email.to_lowercase()); }
        }
        Ok(set)
    })
}

pub fn set_strata(store: &Store, id: &MessageId, strata: &[String]) -> Result<()> {
    store.write(|c| {
        c.execute("DELETE FROM message_strata WHERE message_id = ?1", [id.as_str()])?;
        for s in strata {
            c.execute("INSERT OR IGNORE INTO message_strata (message_id, stratum) VALUES (?1, ?2)", params![id.as_str(), s])?;
        }
        Ok(())
    })
}

pub fn stratum_counts(store: &Store) -> Result<Vec<(String, u64)>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT stratum, count(*) FROM message_strata GROUP BY stratum ORDER BY 2 DESC")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?.collect();
        rows
    })
}

/// Ids in a date window, newest first, for simulation and replay.
pub fn ids_since(store: &Store, since: &chrono::DateTime<chrono::Utc>, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id FROM messages WHERE deleted_at IS NULL AND date >= ?1 ORDER BY date DESC LIMIT ?2")?;
        let rows = st.query_map(params![ts(since), limit], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}
