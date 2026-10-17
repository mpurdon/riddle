//! Messages and threads in a Dig store. Bodies are nullable and fetched
//! lazily; list rows never load them.

use crate::{from_json, from_ts, now_ts, to_json, ts, Store};
use riddle_core::{
    Address, AttachmentMeta, DigId, Message, MessageHeaders, MessageId, MessageSummary, Page,
    PageRequest, ThreadId,
};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension, Row};

fn row_to_message(r: &Row<'_>, dig_id: &DigId) -> rusqlite::Result<Message> {
    Ok(Message {
        id: MessageId(r.get("id")?),
        dig_id: dig_id.clone(),
        thread_id: ThreadId(r.get("thread_id")?),
        provider_id: r.get("provider_id")?,
        subject: r.get("subject")?,
        from: Address {
            email: r.get("from_email")?,
            name: r.get("from_name")?,
        },
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
        has_body: r.get::<_, Option<String>>("body_text")?.is_some()
            || r.get::<_, Option<String>>("body_html")?.is_some(),
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
        c.query_row(
            &format!("SELECT {COLS} FROM messages WHERE id = ?1 AND deleted_at IS NULL"),
            [id.as_str()],
            |r| row_to_message(r, dig_id),
        )
        .optional()
    })
}

pub fn get_many(store: &Store, dig_id: &DigId, ids: &[MessageId]) -> Result<Vec<Message>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(m) = get(store, dig_id, id)? {
            out.push(m);
        }
    }
    Ok(out)
}

pub fn set_body(
    store: &Store,
    id: &MessageId,
    text: Option<&str>,
    html: Option<&str>,
) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE messages SET body_text = ?2, body_html = ?3 WHERE id = ?1",
            params![id.as_str(), text, html],
        )?;
        Ok(())
    })
}

pub fn set_labels(store: &Store, id: &MessageId, labels: &[String]) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE messages SET labels_json = ?2 WHERE id = ?1",
            params![id.as_str(), to_json(&labels)?],
        )?;
        Ok(())
    })
}

pub fn mark_deleted(store: &Store, id: &MessageId) -> Result<()> {
    store.write(|c| {
        c.execute(
            "UPDATE messages SET deleted_at = ?2 WHERE id = ?1",
            params![id.as_str(), now_ts()],
        )?;
        Ok(())
    })
}

pub fn labels_of(store: &Store, id: &MessageId) -> Result<Vec<String>> {
    store.read(|c| {
        Ok(c.query_row(
            "SELECT labels_json FROM messages WHERE id = ?1",
            [id.as_str()],
            |r| from_json::<Vec<String>>(r.get(0)?),
        )
        .optional()?
        .unwrap_or_default())
    })
}

pub fn thread_messages(store: &Store, dig_id: &DigId, thread: &ThreadId) -> Result<Vec<Message>> {
    store.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY date"
        ))?;
        let rows = st
            .query_map([thread.as_str()], |r| row_to_message(r, dig_id))?
            .collect();
        rows
    })
}

pub fn count(store: &Store) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM messages WHERE deleted_at IS NULL",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as u64)
    })
}

pub fn count_since(store: &Store, since: &chrono::DateTime<chrono::Utc>) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM messages WHERE deleted_at IS NULL AND date >= ?1",
            [ts(since)],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as u64)
    })
}

/// Message ids with no current classification, newest first: the Sift backlog.
/// Messages Sift has not decided. A Shadow proposal counts as a decision
/// only while the account is in its Shadow Period (`shadow_decides`), when
/// proposals are all it records; otherwise mail a Mesh in Shadow had only
/// proposed for still needs a live decision.
pub fn unclassified_ids(store: &Store, limit: u32, shadow_decides: bool) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT m.id FROM messages m
             WHERE m.deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM classifications k WHERE k.message_id = m.id AND k.superseded_at IS NULL AND (?2 OR k.shadow = 0))
             ORDER BY m.date DESC LIMIT ?1",
        )?;
        let rows = st.query_map(params![limit, shadow_decides], |r| Ok(MessageId(r.get(0)?)))?.collect();
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
    /// Only messages with a Shadow proposal, and their proposal as the
    /// disposition: the Shadow review list.
    pub shadow_only: bool,
    pub stratum: Option<String>,
    /// The Priority inbox's OR group, ANDed with everything above.
    pub priority: Option<PriorityCriteria>,
    /// `Some(false)` hides messages a newer one of their `+ Replaces`
    /// series superseded, `Some(true)` lists only those; `None` is both.
    pub superseded: Option<bool>,
    /// Only messages at this level or above (the user's own level first).
    pub min_importance: Option<riddle_core::Importance>,
    /// A place in the mail column, resolved for this Dig: a label's ids,
    /// or a Topic's Stratum and same-named label, OR-combined.
    pub any_of: Option<PriorityCriteria>,
}

/// One OR group for the Priority inbox: a message matches when it carries
/// any of these Strata, any of these provider labels, or is one of these
/// ids. The ids are the sources of the open Finds that meet the user's
/// priority criteria; Finds live in the index store, a different database,
/// so the caller gathers them and hands them over. Empty criteria match
/// nothing, so an unconfigured Priority inbox is empty rather than the
/// whole mailbox.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PriorityCriteria {
    pub strata: Vec<String>,
    pub labels: Vec<String>,
    pub message_ids: Vec<MessageId>,
    /// Messages at this level or above (the user's own level first).
    pub min_importance: Option<riddle_core::Importance>,
}

impl PriorityCriteria {
    pub fn is_empty(&self) -> bool {
        self.strata.is_empty()
            && self.labels.is_empty()
            && self.message_ids.is_empty()
            && self.min_importance.is_none()
    }
}

pub fn list(
    store: &Store,
    dig_id: &DigId,
    filter: &MessageFilter,
    page: &PageRequest,
) -> Result<Page<MessageSummary>> {
    search(store, dig_id, filter, &SearchExtras::default(), page)
}

/// Search-only refinements layered on `MessageFilter` (added for
/// `riddle-search`): a prepared FTS5 expression that replaces the quoted
/// `filter.query`, recipient matching, attachment presence, sent-ness and a
/// disposition set.
#[derive(Debug, Default, Clone)]
pub struct SearchExtras {
    /// A complete FTS5 MATCH expression; used instead of `filter.query` when set.
    pub fts: Option<String>,
    pub to: Option<String>,
    pub has_attachment: Option<bool>,
    pub is_sent: Option<bool>,
    pub disposition_in: Vec<String>,
    /// Sender addresses to exclude (`-from:` in a query).
    pub exclude_from: Vec<String>,
}

/// Appends one OR group: EXISTS on the (message_id, stratum) key, json_each
/// over the label array, and an IN list for the Find sources. An empty
/// group matches nothing.
fn or_group(sql: &mut String, args: &mut Vec<Box<dyn rusqlite::ToSql>>, p: &PriorityCriteria) {
    let mut parts: Vec<String> = Vec::new();
    if !p.strata.is_empty() {
        let marks = vec!["?"; p.strata.len()].join(",");
        parts.push(format!("EXISTS (SELECT 1 FROM message_strata ms WHERE ms.message_id = m.id AND ms.stratum IN ({marks}))"));
        for s in &p.strata {
            args.push(Box::new(s.clone()));
        }
    }
    if !p.labels.is_empty() {
        let marks = vec!["?"; p.labels.len()].join(",");
        parts.push(format!(
            "EXISTS (SELECT 1 FROM json_each(m.labels_json) WHERE value IN ({marks}))"
        ));
        for l in &p.labels {
            args.push(Box::new(l.clone()));
        }
    }
    if !p.message_ids.is_empty() {
        // One JSON array parameter, not one `?` per id: SQLite caps
        // bound parameters, and the count query binds the list a
        // second time, so 400 open Finds were 800 parameters a page.
        let ids: Vec<&str> = p.message_ids.iter().map(|id| id.as_str()).collect();
        parts.push("m.id IN (SELECT value FROM json_each(?))".to_string());
        args.push(Box::new(
            serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()),
        ));
    }
    if let Some(i) = p.min_importance {
        parts.push("coalesce(io.importance, k.importance, 1) >= ?".to_string());
        args.push(Box::new(i.rank()));
    }
    if parts.is_empty() {
        sql.push_str(" AND 0");
    } else {
        sql.push_str(&format!(" AND ({})", parts.join(" OR ")));
    }
}

/// Messages and unread messages per Stratum, over live mail.
pub fn strata_counts(store: &Store) -> Result<std::collections::HashMap<String, (u64, u64)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT ms.stratum, count(*),
                    sum(EXISTS (SELECT 1 FROM json_each(m.labels_json) WHERE value = 'UNREAD'))
             FROM message_strata ms JOIN messages m ON m.id = ms.message_id
             WHERE m.deleted_at IS NULL GROUP BY ms.stratum",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64),
            ))
        })?;
        rows.collect()
    })
}

/// Messages and unread messages carrying any of these Strata or labels:
/// a Topic's count, where a message with both the label and the Stratum
/// counts once.
pub fn count_any_of(store: &Store, strata: &[String], labels: &[String]) -> Result<(u64, u64)> {
    if strata.is_empty() && labels.is_empty() {
        return Ok((0, 0));
    }
    let mut sql = String::from(
        "SELECT count(*), coalesce(sum(EXISTS (SELECT 1 FROM json_each(m.labels_json) WHERE value = 'UNREAD')), 0)
         FROM messages m WHERE m.deleted_at IS NULL",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    or_group(
        &mut sql,
        &mut args,
        &PriorityCriteria {
            strata: strata.to_vec(),
            labels: labels.to_vec(),
            ..Default::default()
        },
    );
    store.read(|c| {
        c.query_row(
            &sql,
            rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())),
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )
    })
}

pub fn search(
    store: &Store,
    dig_id: &DigId,
    filter: &MessageFilter,
    extras: &SearchExtras,
    page: &PageRequest,
) -> Result<Page<MessageSummary>> {
    let limit = page.limit();
    let offset = page.offset();
    store.read(|c| {
        let mut sql = String::from(
            "SELECT m.id, m.thread_id, m.subject, m.from_email, m.from_name, m.date, m.snippet, m.labels_json, m.is_sent, m.attachments_json,
                    k.json AS class_json, sr.review AS shadow_review, ms.superseded_by AS superseded_by,
                    coalesce(io.importance, k.importance, 1) AS importance, io.importance IS NOT NULL AS importance_set_by_you
             FROM messages m
             LEFT JOIN classifications k ON k.message_id = m.id AND k.superseded_at IS NULL AND k.shadow = ?
             LEFT JOIN importance_overrides io ON io.message_id = m.id
             LEFT JOIN shadow_reviews sr ON sr.message_id = m.id
             LEFT JOIN message_series ms ON ms.message_id = m.id AND ms.series_key = k.series_key
             WHERE m.deleted_at IS NULL",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(filter.shadow_only as i64)];
        if filter.shadow_only {
            sql.push_str(" AND k.json IS NOT NULL");
        }
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
        match filter.superseded {
            Some(false) => sql.push_str(" AND ms.superseded_by IS NULL"),
            Some(true) => sql.push_str(" AND ms.superseded_by IS NOT NULL"),
            None => {}
        }
        if let Some(i) = filter.min_importance {
            sql.push_str(" AND coalesce(io.importance, k.importance, 1) >= ?");
            args.push(Box::new(i.rank()));
        }
        if let Some(l) = &filter.label {
            sql.push_str(" AND EXISTS (SELECT 1 FROM json_each(m.labels_json) WHERE value = ?)");
            args.push(Box::new(l.clone()));
        }
        if let Some(s) = &filter.stratum {
            sql.push_str(" AND EXISTS (SELECT 1 FROM message_strata ms WHERE ms.message_id = m.id AND ms.stratum = ?)");
            args.push(Box::new(s.clone()));
        }
        for p in [&filter.priority, &filter.any_of].into_iter().flatten() {
            or_group(&mut sql, &mut args, p);
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
        if let Some(raw) = extras.fts.as_ref().filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND m.rowid IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)");
            args.push(Box::new(raw.clone()));
        } else if let Some(q) = filter.query.as_ref().filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND m.rowid IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)");
            args.push(Box::new(fts_query(q)));
        }
        if let Some(t) = &extras.to {
            sql.push_str(" AND (EXISTS (SELECT 1 FROM json_each(m.to_json) WHERE json_extract(value, '$.email') LIKE ?) OR EXISTS (SELECT 1 FROM json_each(m.cc_json) WHERE json_extract(value, '$.email') LIKE ?))");
            let pat = format!("%{}%", t.to_lowercase());
            args.push(Box::new(pat.clone()));
            args.push(Box::new(pat));
        }
        if let Some(h) = extras.has_attachment {
            sql.push_str(if h { " AND json_array_length(m.attachments_json) > 0" } else { " AND json_array_length(m.attachments_json) = 0" });
        }
        if let Some(sent) = extras.is_sent {
            sql.push_str(" AND m.is_sent = ?");
            args.push(Box::new(sent as i64));
        }
        if !extras.disposition_in.is_empty() {
            let marks = vec!["?"; extras.disposition_in.len()].join(",");
            sql.push_str(&format!(" AND k.disposition_kind IN ({marks})"));
            for d in &extras.disposition_in {
                args.push(Box::new(d.clone()));
            }
        }
        for x in &extras.exclude_from {
            sql.push_str(" AND m.from_email NOT LIKE ?");
            args.push(Box::new(format!("%{}%", x.to_lowercase())));
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
                    shadow: filter.shadow_only && class.is_some(),
                    shadow_review: r
                        .get::<_, Option<String>>("shadow_review")?
                        .and_then(|v| riddle_core::ShadowReview::parse(&v)),
                    superseded_by: r.get::<_, Option<String>>("superseded_by")?.map(MessageId),
                    importance: riddle_core::Importance::from_rank(r.get("importance")?),
                    importance_set_by_you: r.get("importance_set_by_you")?,
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
        .map(|(i, t)| {
            if i + 1 == n {
                format!("\"{t}\"*")
            } else {
                format!("\"{t}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Distinct sender addresses the user has replied to, for facts.
pub fn replied_to_senders(store: &Store) -> Result<std::collections::HashSet<String>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT DISTINCT to_json FROM messages WHERE is_sent = 1 AND deleted_at IS NULL",
        )?;
        let rows = st.query_map([], |r| from_json::<Vec<Address>>(r.get(0)?))?;
        let mut set = std::collections::HashSet::new();
        for row in rows {
            for a in row? {
                set.insert(a.email.to_lowercase());
            }
        }
        Ok(set)
    })
}

pub fn set_strata(store: &Store, id: &MessageId, strata: &[String]) -> Result<()> {
    store.write(|c| {
        c.execute(
            "DELETE FROM message_strata WHERE message_id = ?1",
            [id.as_str()],
        )?;
        for s in strata {
            c.execute(
                "INSERT OR IGNORE INTO message_strata (message_id, stratum) VALUES (?1, ?2)",
                params![id.as_str(), s],
            )?;
        }
        Ok(())
    })
}

pub fn stratum_counts(store: &Store) -> Result<Vec<(String, u64)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT stratum, count(*) FROM message_strata GROUP BY stratum ORDER BY 2 DESC",
        )?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect();
        rows
    })
}

/// Ids in a date window, newest first, for simulation and replay.
pub fn ids_since(
    store: &Store,
    since: &chrono::DateTime<chrono::Utc>,
    limit: u32,
) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id FROM messages WHERE deleted_at IS NULL AND date >= ?1 ORDER BY date DESC LIMIT ?2")?;
        let rows = st.query_map(params![ts(since), limit], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}

/// Every message received from one address, newest first.
pub fn ids_from_sender(store: &Store, email: &str) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id FROM messages WHERE lower(from_email) = lower(?1) AND deleted_at IS NULL AND is_sent = 0 ORDER BY date DESC")?;
        let rows = st.query_map([email], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}

/// Full rows without bodies, newest first, for aggregation passes (People,
/// resync reconciliation) that must not load 100k bodies.
pub fn page_headers(
    store: &Store,
    dig_id: &DigId,
    offset: u64,
    limit: u32,
) -> Result<Vec<Message>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT id, provider_id, thread_id, subject, from_email, from_name, to_json, cc_json, bcc_json, date, snippet, labels_json, headers_json, attachments_json, size_bytes, is_sent, NULL AS body_text, NULL AS body_html, synced_at
             FROM messages WHERE deleted_at IS NULL ORDER BY date DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = st.query_map(params![limit, offset as i64], |r| row_to_message(r, dig_id))?.collect();
        rows
    })
}

/// Messages whose body has not been fetched, newest first (§53: bodies lazily).
pub fn ids_without_body(store: &Store, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT id FROM messages WHERE deleted_at IS NULL AND body_text IS NULL AND body_html IS NULL ORDER BY date DESC LIMIT ?1")?;
        let rows = st.query_map([limit], |r| Ok(MessageId(r.get(0)?)))?.collect();
        rows
    })
}

pub fn count_without_body(store: &Store) -> Result<u64> {
    store.read(|c| c.query_row("SELECT count(*) FROM messages WHERE deleted_at IS NULL AND body_text IS NULL AND body_html IS NULL", [], |r| r.get::<_, i64>(0)).map(|n| n as u64))
}

/// Every distinct sender address (received mail only).
pub fn distinct_senders(store: &Store) -> Result<Vec<String>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT DISTINCT from_email FROM messages WHERE deleted_at IS NULL AND is_sent = 0",
        )?;
        let rows = st.query_map([], |r| r.get(0))?.collect();
        rows
    })
}

/// Received messages per sender address since a moment, lowercase, for the
/// sender review: one query for every sender in the window.
pub fn sender_counts_since(
    store: &Store,
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, u64)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT lower(from_email), count(*) FROM messages WHERE deleted_at IS NULL AND is_sent = 0 AND date >= ?1 GROUP BY 1",
        )?;
        let rows = st
            .query_map([ts(since)], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect();
        rows
    })
}

/// Sender addresses in a set of threads, plus recipients of the user's sent
/// mail in them, for incremental People updates.
pub fn addresses_in_threads(
    store: &Store,
    threads: &[ThreadId],
) -> Result<std::collections::HashSet<String>> {
    let mut set = std::collections::HashSet::new();
    for t in threads {
        store.read(|c| {
            let mut st = c.prepare("SELECT from_email, to_json, cc_json, is_sent FROM messages WHERE thread_id = ?1 AND deleted_at IS NULL")?;
            let rows = st.query_map([t.as_str()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            for row in rows {
                let (from, to, cc, sent) = row?;
                set.insert(from.to_lowercase());
                if sent != 0 {
                    for a in from_json::<Vec<Address>>(to)?.into_iter().chain(from_json::<Vec<Address>>(cc)?) {
                        set.insert(a.email.to_lowercase());
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(set)
}

/// Per-address aggregates for People (§2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AddressStats {
    pub messages_from: u64,
    /// Sent by the user to this address (To or Cc).
    pub messages_to: u64,
    pub first_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_at: Option<chrono::DateTime<chrono::Utc>>,
    pub name: Option<String>,
    /// Received messages with list headers, bulk precedence or auto-submitted.
    pub automated: u64,
    pub mailing_list: u64,
    pub auth_failures: u64,
    pub spam_verdicts: u64,
}

/// When mail from `email` arrived since `since`, oldest first; what a
/// sender's cadence is classified from.
pub fn arrivals_from(
    store: &Store,
    email: &str,
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<chrono::DateTime<chrono::Utc>>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT date FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND is_sent = 0 AND date >= ?2 ORDER BY date",
        )?;
        let rows = st
            .query_map(params![email, ts(since)], |r| Ok(crate::from_ts(r.get(0)?)))?
            .collect();
        rows
    })
}

pub fn address_stats(store: &Store, email: &str) -> Result<AddressStats> {
    let email = email.to_lowercase();
    store.read(|c| {
        let (messages_from, first, last): (i64, Option<i64>, Option<i64>) = c.query_row(
            "SELECT count(*), min(date), max(date) FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND is_sent = 0",
            [&email], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let (messages_to, first_to, last_to): (i64, Option<i64>, Option<i64>) = c.query_row(
            "SELECT count(*), min(date), max(date) FROM messages m WHERE m.is_sent = 1 AND m.deleted_at IS NULL AND (
               EXISTS (SELECT 1 FROM json_each(m.to_json) WHERE lower(json_extract(value, '$.email')) = ?1)
               OR EXISTS (SELECT 1 FROM json_each(m.cc_json) WHERE lower(json_extract(value, '$.email')) = ?1))",
            [&email], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let name: Option<String> = c.query_row(
            "SELECT from_name FROM messages WHERE from_email = ?1 AND from_name IS NOT NULL AND from_name != '' AND deleted_at IS NULL ORDER BY date DESC LIMIT 1",
            [&email], |r| r.get(0),
        ).optional()?;
        let automated: i64 = c.query_row(
            "SELECT count(*) FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND is_sent = 0 AND (
               json_extract(headers_json, '$.listId') IS NOT NULL OR json_extract(headers_json, '$.listUnsubscribe') IS NOT NULL
               OR json_extract(headers_json, '$.autoSubmitted') IS NOT NULL OR lower(json_extract(headers_json, '$.precedence')) IN ('bulk', 'list', 'junk'))",
            [&email], |r| r.get(0),
        )?;
        let mailing_list: i64 = c.query_row(
            "SELECT count(*) FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND is_sent = 0 AND (json_extract(headers_json, '$.listId') IS NOT NULL OR lower(json_extract(headers_json, '$.precedence')) = 'list')",
            [&email], |r| r.get(0),
        )?;
        let auth_failures: i64 = c.query_row(
            "SELECT count(*) FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND (
               json_extract(headers_json, '$.authentication.spf') = 'fail' OR json_extract(headers_json, '$.authentication.dkim') = 'fail' OR json_extract(headers_json, '$.authentication.dmarc') = 'fail')",
            [&email], |r| r.get(0),
        )?;
        let spam_verdicts: i64 = c.query_row(
            "SELECT count(*) FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND EXISTS (SELECT 1 FROM json_each(labels_json) WHERE value = 'SPAM')",
            [&email], |r| r.get(0),
        )?;
        let min_of = |a: Option<i64>, b: Option<i64>| match (a, b) { (Some(x), Some(y)) => Some(x.min(y)), (x, None) => x, (None, y) => y };
        let max_of = |a: Option<i64>, b: Option<i64>| match (a, b) { (Some(x), Some(y)) => Some(x.max(y)), (x, None) => x, (None, y) => y };
        Ok(AddressStats {
            messages_from: messages_from as u64,
            messages_to: messages_to as u64,
            first_at: min_of(first, first_to).map(from_ts),
            last_at: max_of(last, last_to).map(from_ts),
            name,
            automated: automated as u64,
            mailing_list: mailing_list as u64,
            auth_failures: auth_failures as u64,
            spam_verdicts: spam_verdicts as u64,
        })
    })
}

/// Provider ids and current labels for every live message dated on or after
/// `since`, for resync reconciliation (§15) without loading rows.
pub fn provider_labels_since(
    store: &Store,
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, MessageId, Vec<String>)>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT provider_id, id, labels_json FROM messages WHERE deleted_at IS NULL AND date >= ?1")?;
        let rows = st.query_map([ts(since)], |r| Ok((r.get(0)?, MessageId(r.get(1)?), from_json::<Vec<String>>(r.get(2)?)?)))?.collect();
        rows
    })
}

pub fn get_by_provider_id(
    store: &Store,
    dig_id: &DigId,
    provider_id: &str,
) -> Result<Option<Message>> {
    store.read(|c| {
        c.query_row(
            &format!("SELECT {COLS} FROM messages WHERE provider_id = ?1 AND deleted_at IS NULL"),
            [provider_id],
            |r| row_to_message(r, dig_id),
        )
        .optional()
    })
}

/// Rows without bodies matching a set of ids, in one query.
pub fn summaries_for_ids(store: &Store, dig_id: &DigId, ids: &[MessageId]) -> Result<Vec<Message>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let m = store.read(|c| {
            c.query_row(
                "SELECT id, provider_id, thread_id, subject, from_email, from_name, to_json, cc_json, bcc_json, date, snippet, labels_json, headers_json, attachments_json, size_bytes, is_sent, NULL AS body_text, NULL AS body_html, synced_at FROM messages WHERE id = ?1 AND deleted_at IS NULL",
                [id.as_str()], |r| row_to_message(r, dig_id),
            ).optional()
        })?;
        if let Some(m) = m {
            out.push(m);
        }
    }
    Ok(out)
}

/// Who wrote in a thread before a moment: each earlier message's sender and
/// whether the user sent it. For `Facts::known_thread`.
pub fn thread_senders_before(
    store: &Store,
    thread_id: &ThreadId,
    before: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, bool)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT from_email, is_sent FROM messages WHERE thread_id = ?1 AND date < ?2 AND deleted_at IS NULL",
        )?;
        let rows = st
            .query_map(params![thread_id.as_str(), ts(before)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?
            .collect();
        rows
    })
}

/// The Strata on the other messages of a thread, for a reply the user sent
/// in it (`message_strata` holds live decisions only).
pub fn thread_strata(
    store: &Store,
    thread_id: &ThreadId,
    except: &MessageId,
) -> Result<Vec<String>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT DISTINCT ms.stratum FROM message_strata ms JOIN messages m ON m.id = ms.message_id
             WHERE m.thread_id = ?1 AND m.id <> ?2 AND m.deleted_at IS NULL ORDER BY ms.stratum",
        )?;
        let rows = st.query_map(params![thread_id.as_str(), except.as_str()], |r| r.get::<_, String>(0))?.collect();
        rows
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreKind;

    fn message(dig: &DigId, n: u32, labels: &[&str]) -> Message {
        let now = chrono::Utc::now() - chrono::Duration::minutes(n as i64);
        Message {
            id: MessageId::for_provider(dig, &format!("p{n}")),
            dig_id: dig.clone(),
            thread_id: ThreadId::for_provider(dig, &format!("t{n}")),
            provider_id: format!("p{n}"),
            subject: format!("Subject {n}"),
            from: Address::new(format!("sender{n}@example.com"), None),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            date: now,
            snippet: String::new(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            headers: MessageHeaders::default(),
            attachments: vec![],
            size_bytes: 0,
            is_sent: false,
            has_body: false,
            body_text: None,
            body_html: None,
            synced_at: now,
        }
    }

    fn ids(page: &Page<MessageSummary>) -> Vec<String> {
        let mut v: Vec<String> = page
            .items
            .iter()
            .map(|m| m.id.as_str().to_string())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn the_priority_group_surfaces_a_label_a_stratum_or_a_find_source_and_nothing_else() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        let starred = message(&dig, 1, &["INBOX", "STARRED"]);
        let finance = message(&dig, 2, &["INBOX"]);
        let found = message(&dig, 3, &["INBOX"]);
        let plain = message(&dig, 4, &["INBOX", "IMPORTANT"]);
        for m in [&starred, &finance, &found, &plain] {
            upsert(&store, m).unwrap();
        }
        set_strata(&store, &finance.id, &["Finance".to_string()]).unwrap();
        set_strata(&store, &plain.id, &["Travel".to_string()]).unwrap();
        let filter = MessageFilter {
            priority: Some(PriorityCriteria {
                strata: vec!["Finance".into()],
                labels: vec!["STARRED".into()],
                message_ids: vec![found.id.clone()],
                min_importance: None,
            }),
            ..Default::default()
        };
        let page = list(&store, &dig, &filter, &PageRequest::default()).unwrap();
        let mut want = vec![
            starred.id.as_str().to_string(),
            finance.id.as_str().to_string(),
            found.id.as_str().to_string(),
        ];
        want.sort();
        assert_eq!(ids(&page), want);
        assert_eq!(page.total, Some(3));

        // The group is ANDed with the other filter fields.
        let narrowed = MessageFilter {
            label: Some("INBOX".into()),
            from: Some("sender1".into()),
            ..filter.clone()
        };
        let page = list(&store, &dig, &narrowed, &PageRequest::default()).unwrap();
        assert_eq!(ids(&page), vec![starred.id.as_str().to_string()]);

        // Empty criteria match nothing, not everything.
        let empty = MessageFilter {
            priority: Some(PriorityCriteria::default()),
            ..Default::default()
        };
        let page = list(&store, &dig, &empty, &PageRequest::default()).unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.total, Some(0));
    }
}
