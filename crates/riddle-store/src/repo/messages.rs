//! Messages and threads in a Dig store. Bodies are nullable and fetched
//! lazily; list rows never load them.
//!
//! The Sift backlog is read from the row: `sifted_at` (and
//! `shadow_sifted_at` for a Shadow proposal) mirror the message's current
//! classification through triggers on the classifications table (migration
//! 0008), so `unclassified_ids` is a probe of the `messages_unsifted`
//! partial index rather than a probe of `classifications` per message from
//! the newest down to the oldest, which is where the unsifted ones are. A
//! message sent back to Sift (`supersede_current`, `supersede_by_mesh`,
//! `supersede_sift_decisions`) looks unclassified again through the same
//! triggers; nothing clears the markers by hand. Sender addresses are
//! stored lowercase so the sender queries use `messages_from` instead of
//! `lower(from_email)` (17 Oct 2026).

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
            m.id.as_str(), m.provider_id, m.thread_id.as_str(), m.subject, m.from.email.to_lowercase(), m.from.name,
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

/// Rows with bodies for a set of ids, one query, in the order asked.
pub fn get_many(store: &Store, dig_id: &DigId, ids: &[MessageId]) -> Result<Vec<Message>> {
    rows_for_ids(store, dig_id, ids, COLS)
}

/// `COLS` without the bodies, for lists and summaries.
const SUMMARY_COLS: &str = "id, provider_id, thread_id, subject, from_email, from_name, to_json, cc_json, bcc_json, date, snippet, labels_json, headers_json, attachments_json, size_bytes, is_sent, NULL AS body_text, NULL AS body_html, synced_at";

fn rows_for_ids(
    store: &Store,
    dig_id: &DigId,
    ids: &[MessageId],
    cols: &str,
) -> Result<Vec<Message>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let json = crate::ids_json(ids.iter().map(|id| id.as_str()));
    let mut rows: Vec<Message> = store.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {cols} FROM messages WHERE deleted_at IS NULL AND id IN (SELECT value FROM json_each(?1))"
        ))?;
        let rows = st
            .query_map([json], |r| row_to_message(r, dig_id))?
            .collect();
        rows
    })?;
    // In the order asked: callers walk their ids.
    let order: std::collections::HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    rows.sort_by_key(|m| order.get(m.id.as_str()).copied().unwrap_or(usize::MAX));
    Ok(rows)
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

/// A thread's messages in date order with bodies only on the newest
/// `newest`: Thread State scans the newest for asks and commitments and
/// needs only senders, dates and the count from the rest. Every body of
/// a hundred-message thread was loaded and normalised each time one
/// message of it landed (19 Oct 2026).
pub fn thread_messages_bodies_newest(
    store: &Store,
    dig_id: &DigId,
    thread: &ThreadId,
    newest: usize,
) -> Result<Vec<Message>> {
    let mut rows: Vec<Message> = store.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {SUMMARY_COLS} FROM messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY date"
        ))?;
        let rows = st
            .query_map([thread.as_str()], |r| row_to_message(r, dig_id))?
            .collect();
        rows
    })?;
    let from = rows.len().saturating_sub(newest);
    let ids: Vec<MessageId> = rows[from..].iter().map(|m| m.id.clone()).collect();
    let mut bodies: std::collections::HashMap<MessageId, Message> = get_many(store, dig_id, &ids)?
        .into_iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    for row in &mut rows[from..] {
        if let Some(full) = bodies.remove(&row.id) {
            *row = full;
        }
    }
    Ok(rows)
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
    // Both forms imply the `messages_unsifted` index's WHERE clause.
    let shadow = if shadow_decides {
        " AND shadow_sifted_at IS NULL"
    } else {
        ""
    };
    store.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL{shadow}
             ORDER BY date DESC LIMIT ?1"
        ))?;
        let rows = st
            .query_map([limit], |r| Ok(MessageId(r.get(0)?)))?
            .collect();
        rows
    })
}

/// Messages with no current classification of either kind.
pub fn unclassified_count(store: &Store) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND shadow_sifted_at IS NULL",
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
                    held_reason: class
                        .as_ref()
                        .filter(|k| k.disposition.kind() == riddle_core::DispositionKind::Unresolved)
                        .map(|k| k.reason.clone()),
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
    let email = email.to_lowercase();
    store.read(|c| {
        let mut st = c.prepare("SELECT id FROM messages WHERE from_email = ?1 AND deleted_at IS NULL AND is_sent = 0 ORDER BY date DESC")?;
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
            "SELECT from_email, count(*) FROM messages WHERE deleted_at IS NULL AND is_sent = 0 AND date >= ?1 GROUP BY from_email",
        )?;
        let rows = st
            .query_map([ts(since)], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect();
        rows
    })
}

/// Received messages per Gmail category since a moment, the category
/// lowercase without its `CATEGORY_` prefix (`promotions`, `updates`), for
/// pack detection: one query over the label arrays for every category.
pub fn category_counts_since(
    store: &Store,
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, u64)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT lower(substr(j.value, 10)), count(*) FROM messages m, json_each(m.labels_json) j
             WHERE m.deleted_at IS NULL AND m.is_sent = 0 AND m.date >= ?1 AND upper(j.value) LIKE 'CATEGORY\\_%' ESCAPE '\\'
             GROUP BY lower(substr(j.value, 10))",
        )?;
        let rows = st
            .query_map([ts(since)], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect();
        rows
    })
}

/// Each sender's newest messages in the window, up to `per_sender`, from
/// one query: what the invoice count reads, so a sender with years of
/// promotions and a few invoices among them is not an invoice sender.
pub fn sender_recent_ids(
    store: &Store,
    since: &chrono::DateTime<chrono::Utc>,
    per_sender: usize,
) -> Result<std::collections::HashMap<String, Vec<MessageId>>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT from_email, id FROM messages WHERE deleted_at IS NULL AND is_sent = 0 AND date >= ?1 ORDER BY date DESC",
        )?;
        let mut out: std::collections::HashMap<String, Vec<MessageId>> =
            std::collections::HashMap::new();
        for row in st.query_map([ts(since)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (sender, id) = row?;
            let ids = out.entry(sender).or_default();
            if ids.len() < per_sender {
                ids.push(MessageId::from(id.as_str()));
            }
        }
        Ok(out)
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

/// Rows without bodies matching a set of ids, in one query, in the order
/// asked.
pub fn summaries_for_ids(store: &Store, dig_id: &DigId, ids: &[MessageId]) -> Result<Vec<Message>> {
    rows_for_ids(store, dig_id, ids, SUMMARY_COLS)
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

/// The current live decision of every other message in a thread: `(message
/// id, disposition kind)`, oldest first. The `previous_classifications`
/// Context slice loaded the thread with bodies and asked for each sibling's
/// current classification one read at a time, for every message sifted
/// (17 Oct 2026); it needs the kinds alone.
pub fn thread_decisions(
    store: &Store,
    thread_id: &ThreadId,
    except: &MessageId,
) -> Result<Vec<(MessageId, String)>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT k.message_id, k.disposition_kind FROM classifications k JOIN messages m ON m.id = k.message_id
             WHERE m.thread_id = ?1 AND m.id <> ?2 AND m.deleted_at IS NULL AND k.shadow = 0 AND k.superseded_at IS NULL
             ORDER BY m.date",
        )?;
        let rows = st
            .query_map(params![thread_id.as_str(), except.as_str()], |r| {
                Ok((MessageId(r.get(0)?), r.get(1)?))
            })?
            .collect();
        rows
    })
}

/// When the oldest live message was sent: where a Dig's synced span starts,
/// for a store synced before the first sync recorded its window start
/// (§53). One probe of the date index; `None` for an empty Dig.
pub fn oldest_date(store: &Store) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    store.read(|c| {
        c.query_row(
            "SELECT min(date) FROM messages WHERE deleted_at IS NULL",
            [],
            |r| r.get::<_, Option<i64>>(0),
        )
        .map(|t| t.map(from_ts))
    })
}

/// Messages with no current classification of either kind dated on or
/// after `since`: the backlog of the sync window alone, so an account whose
/// recent mail is sorted reads as caught up while the history backfill's
/// older mail waits its turn (§53). An index range on `messages_unsifted`.
pub fn unclassified_count_since(
    store: &Store,
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND shadow_sifted_at IS NULL AND date >= ?1",
            [ts(since)], |r| r.get::<_, i64>(0),
        ).map(|n| n as u64)
    })
}

/// The Sift backlog, newest first, preferring messages whose body has
/// landed. Bodies arrive a cycle behind the listing, and a message judged
/// on its snippet is judged for good (§53), so a body-less message fills
/// the batch only when it is older than `bodyless_before`: the start of the
/// body window, mail the body fetch does not cover and that is judged on
/// what it has. Once nothing is left to fetch (`count_without_body` is 0,
/// a missing body stored as "") this is `unclassified_ids`. Both queries
/// walk the `messages_unsifted` index.
pub fn unclassified_ids_preferring_bodies(
    store: &Store,
    limit: u32,
    shadow_decides: bool,
    bodyless_before: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<MessageId>> {
    let shadow = if shadow_decides {
        " AND shadow_sifted_at IS NULL"
    } else {
        ""
    };
    store.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL{shadow}
             AND (body_text IS NOT NULL OR body_html IS NOT NULL)
             ORDER BY date DESC LIMIT ?1"
        ))?;
        let mut ids: Vec<MessageId> = st
            .query_map([limit], |r| Ok(MessageId(r.get(0)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let room = (limit as usize).saturating_sub(ids.len());
        if let (Some(before), true) = (bodyless_before, room > 0) {
            let mut st = c.prepare(&format!(
                "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL{shadow}
                 AND body_text IS NULL AND body_html IS NULL AND date < ?1
                 ORDER BY date DESC LIMIT ?2"
            ))?;
            let older = st
                .query_map(params![ts(&before), room as u32], |r| {
                    Ok(MessageId(r.get(0)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.extend(older);
        }
        Ok(ids)
    })
}

/// Each live message's thread and labels for a set of ids, from one query
/// and without bodies: what applying a label change needs. The incremental
/// sync read the whole row, body included, per changed message.
pub fn labels_for_ids(
    store: &Store,
    ids: &[MessageId],
) -> Result<Vec<(MessageId, ThreadId, Vec<String>)>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let json = crate::ids_json(ids.iter().map(|id| id.as_str()));
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT id, thread_id, labels_json FROM messages
             WHERE deleted_at IS NULL AND id IN (SELECT value FROM json_each(?1))",
        )?;
        let rows = st
            .query_map([json], |r| {
                Ok((
                    MessageId(r.get(0)?),
                    ThreadId(r.get(1)?),
                    from_json::<Vec<String>>(r.get(2)?)?,
                ))
            })?
            .collect();
        rows
    })
}

/// Several messages' labels in one transaction.
pub fn set_labels_many(store: &Store, rows: &[(MessageId, Vec<String>)]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    store.write(|c| {
        let mut st = c.prepare("UPDATE messages SET labels_json = ?2 WHERE id = ?1")?;
        for (id, labels) in rows {
            st.execute(params![id.as_str(), to_json(labels)?])?;
        }
        Ok(())
    })
}

/// `address_stats` for a set of addresses (lowercase) in three queries:
/// one conditional-sum pass over the set's mail, one over the user's sent
/// mail to them with the recipients exploded once, and one for the newest
/// display name. The per-address version ran seven queries each, with
/// `json_extract` on every header, for every address a sync page touched
/// (17 Oct 2026). Addresses with no mail are left out.
pub fn address_stats_many(
    store: &Store,
    emails: &[String],
) -> Result<std::collections::HashMap<String, AddressStats>> {
    let mut out = std::collections::HashMap::new();
    if emails.is_empty() {
        return Ok(out);
    }
    let set = serde_json::to_string(emails).unwrap_or_else(|_| "[]".into());
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT from_email,
                    sum(is_sent = 0),
                    min(CASE WHEN is_sent = 0 THEN date END),
                    max(CASE WHEN is_sent = 0 THEN date END),
                    sum(is_sent = 0 AND (
                      json_extract(headers_json, '$.listId') IS NOT NULL OR json_extract(headers_json, '$.listUnsubscribe') IS NOT NULL
                      OR json_extract(headers_json, '$.autoSubmitted') IS NOT NULL OR lower(json_extract(headers_json, '$.precedence')) IN ('bulk', 'list', 'junk'))),
                    sum(is_sent = 0 AND (json_extract(headers_json, '$.listId') IS NOT NULL OR lower(json_extract(headers_json, '$.precedence')) = 'list')),
                    sum(json_extract(headers_json, '$.authentication.spf') = 'fail' OR json_extract(headers_json, '$.authentication.dkim') = 'fail' OR json_extract(headers_json, '$.authentication.dmarc') = 'fail'),
                    sum(EXISTS (SELECT 1 FROM json_each(labels_json) WHERE value = 'SPAM'))
             FROM messages WHERE deleted_at IS NULL AND from_email IN (SELECT value FROM json_each(?1))
             GROUP BY from_email",
        )?;
        for row in st.query_map([&set], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                r.get::<_, Option<i64>>(6)?.unwrap_or(0),
                r.get::<_, Option<i64>>(7)?.unwrap_or(0),
            ))
        })? {
            let (email, from, first, last, automated, list, auth, spam) = row?;
            let s: &mut AddressStats = out.entry(email).or_default();
            s.messages_from = from as u64;
            s.first_at = first.map(from_ts);
            s.last_at = last.map(from_ts);
            s.automated = automated as u64;
            s.mailing_list = list as u64;
            s.auth_failures = auth as u64;
            s.spam_verdicts = spam as u64;
        }
        let mut st = c.prepare(
            "SELECT rcpt, count(*), min(date), max(date) FROM (
               SELECT m.id, m.date, lower(json_extract(j.value, '$.email')) AS rcpt
               FROM messages m, json_each(m.to_json) j WHERE m.is_sent = 1 AND m.deleted_at IS NULL
               UNION
               SELECT m.id, m.date, lower(json_extract(j.value, '$.email'))
               FROM messages m, json_each(m.cc_json) j WHERE m.is_sent = 1 AND m.deleted_at IS NULL
             ) WHERE rcpt IN (SELECT value FROM json_each(?1)) GROUP BY rcpt",
        )?;
        for row in st.query_map([&set], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })? {
            let (email, to, first_to, last_to) = row?;
            let s: &mut AddressStats = out.entry(email).or_default();
            s.messages_to = to as u64;
            let first_to = first_to.map(from_ts);
            let last_to = last_to.map(from_ts);
            s.first_at = match (s.first_at, first_to) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            s.last_at = match (s.last_at, last_to) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
        let mut st = c.prepare(
            "SELECT from_email, from_name FROM messages
             WHERE deleted_at IS NULL AND from_name IS NOT NULL AND from_name != ''
               AND from_email IN (SELECT value FROM json_each(?1))
             ORDER BY date",
        )?;
        for row in st.query_map([&set], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (email, name) = row?;
            // Oldest first, so the last one kept is the newest.
            if let Some(s) = out.get_mut(&email) {
                s.name = Some(name);
            }
        }
        Ok(())
    })?;
    Ok(out)
}

/// `arrivals_from` for a set of addresses (lowercase) in one query, each
/// address's arrivals oldest first.
pub fn arrivals_from_many(
    store: &Store,
    emails: &[String],
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<std::collections::HashMap<String, Vec<chrono::DateTime<chrono::Utc>>>> {
    let mut out: std::collections::HashMap<String, Vec<chrono::DateTime<chrono::Utc>>> =
        std::collections::HashMap::new();
    if emails.is_empty() {
        return Ok(out);
    }
    let set = serde_json::to_string(emails).unwrap_or_else(|_| "[]".into());
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT from_email, date FROM messages
             WHERE deleted_at IS NULL AND is_sent = 0 AND date >= ?1
               AND from_email IN (SELECT value FROM json_each(?2))
             ORDER BY date",
        )?;
        for row in st.query_map(params![ts(since), &set], |r| {
            Ok((r.get::<_, String>(0)?, crate::from_ts(r.get(1)?)))
        })? {
            let (email, at) = row?;
            out.entry(email).or_default().push(at);
        }
        Ok(())
    })?;
    Ok(out)
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

    fn decided(m: &Message, shadow: bool) -> riddle_core::Classification {
        riddle_core::Classification {
            id: riddle_core::ClassificationId::new(),
            message_id: m.id.clone(),
            dig_id: m.dig_id.clone(),
            importance: riddle_core::Importance::default_for(&riddle_core::Disposition::Spoil),
            disposition: riddle_core::Disposition::Spoil,
            strata: vec![],
            notify: false,
            mechanism: riddle_core::DecisionMechanism::Mesh,
            mesh_id: None,
            mesh_version: None,
            matched_rule: None,
            confidence: None,
            reason: "test".into(),
            trace_id: riddle_core::TraceId::new(),
            shadow,
            created_at: chrono::Utc::now(),
            superseded_at: None,
            series: None,
        }
    }

    fn markers(store: &Store, id: &MessageId) -> (Option<i64>, Option<i64>) {
        store
            .read(|c| {
                c.query_row(
                    "SELECT sifted_at, shadow_sifted_at FROM messages WHERE id = ?1",
                    [id.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .unwrap()
    }

    fn backlog(store: &Store, shadow_decides: bool) -> Vec<String> {
        unclassified_ids(store, 10, shadow_decides)
            .unwrap()
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    #[test]
    fn sifted_at_follows_the_current_classification_and_the_backlog_skips_sifted_rows() {
        use crate::repo::classifications as k;
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        let (m1, m2, m3) = (
            message(&dig, 1, &[]),
            message(&dig, 2, &[]),
            message(&dig, 3, &[]),
        );
        for m in [&m1, &m2, &m3] {
            upsert(&store, m).unwrap();
        }
        let (a, b, c) = (m1.id.as_str(), m2.id.as_str(), m3.id.as_str());
        assert_eq!(backlog(&store, false), [a, b, c]);
        assert_eq!(unclassified_count(&store).unwrap(), 3);

        // A live decision marks the row; the backlog and the count skip it.
        let decision = decided(&m2, false);
        k::record(&store, &decision).unwrap();
        assert_eq!(
            markers(&store, &m2.id),
            (Some(ts(&decision.created_at)), None)
        );
        assert_eq!(backlog(&store, false), [a, c]);
        assert_eq!(unclassified_count(&store).unwrap(), 2);

        // A Shadow proposal counts only while the account is in Shadow.
        k::record(&store, &decided(&m3, true)).unwrap();
        assert!(markers(&store, &m3.id).1.is_some());
        assert_eq!(backlog(&store, false), [a, c]);
        assert_eq!(backlog(&store, true), [a]);
        assert_eq!(unclassified_count(&store).unwrap(), 1);

        // Sent back to Sift (one message, or every Sift decision at once),
        // the row looks unclassified again; a resync leaves the marker.
        k::supersede_current(&store, &m2.id).unwrap();
        assert_eq!(markers(&store, &m2.id), (None, None));
        assert_eq!(backlog(&store, false), [a, b, c]);
        k::record(&store, &decided(&m1, false)).unwrap();
        upsert(&store, &m1).unwrap();
        assert_eq!(backlog(&store, false), [b, c]);
        assert_eq!(k::supersede_sift_decisions(&store).unwrap(), 1);
        assert_eq!(backlog(&store, false), [a, b, c]);
        k::retire_shadow(&store, &m3.id).unwrap();
        assert_eq!(markers(&store, &m3.id), (None, None));
        assert_eq!(unclassified_count(&store).unwrap(), 3);
    }

    #[test]
    fn the_backlog_body_and_sender_queries_use_their_indexes() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let plan = |sql: &str| -> String {
            store
                .read(|c| {
                    let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
                    let rows = st
                        .query_map([], |r| r.get::<_, String>(3))?
                        .collect::<rusqlite::Result<Vec<String>>>()?;
                    Ok(rows.join("\n"))
                })
                .unwrap()
        };
        for sql in [
            "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL ORDER BY date DESC LIMIT 10",
            "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND shadow_sifted_at IS NULL ORDER BY date DESC LIMIT 10",
            "SELECT count(*) FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND shadow_sifted_at IS NULL",
            "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND (body_text IS NOT NULL OR body_html IS NOT NULL) ORDER BY date DESC LIMIT 10",
            "SELECT count(*) FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND shadow_sifted_at IS NULL AND date >= 5",
        ] {
            let p = plan(sql);
            assert!(p.contains("messages_unsifted"), "{sql}\n{p}");
        }
        // The body-less fallback of the Sift batch: either partial index
        // bounds it to the few rows that qualify.
        let sql = "SELECT id FROM messages WHERE sifted_at IS NULL AND deleted_at IS NULL AND body_text IS NULL AND body_html IS NULL AND date < 5 ORDER BY date DESC LIMIT 10";
        let p = plan(sql);
        assert!(
            p.contains("messages_unsifted") || p.contains("messages_without_body"),
            "{sql}\n{p}"
        );
        for sql in [
            "SELECT id FROM messages WHERE deleted_at IS NULL AND body_text IS NULL AND body_html IS NULL ORDER BY date DESC LIMIT 10",
            "SELECT count(*) FROM messages WHERE deleted_at IS NULL AND body_text IS NULL AND body_html IS NULL",
        ] {
            let p = plan(sql);
            assert!(p.contains("messages_without_body"), "{sql}\n{p}");
        }
        let p = plan("SELECT id FROM messages WHERE from_email = 'a@b.c' AND deleted_at IS NULL AND is_sent = 0 ORDER BY date DESC");
        assert!(p.contains("messages_from"), "{p}");
    }

    #[test]
    fn a_mixed_case_sender_is_stored_and_found_by_its_lowercase_address() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        let since = chrono::Utc::now() - chrono::Duration::minutes(100);
        for (n, from) in [(1, "Billing@Acme.example"), (2, "billing@acme.example")] {
            let mut m = message(&dig, n, &[]);
            // Built without `Address::new`, as a provider might.
            m.from = Address {
                email: from.into(),
                name: None,
            };
            upsert(&store, &m).unwrap();
        }
        let stored = get(&store, &dig, &MessageId::for_provider(&dig, "p1"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.from.email, "billing@acme.example");
        for asked in ["billing@acme.example", "BILLING@ACME.EXAMPLE"] {
            assert_eq!(ids_from_sender(&store, asked).unwrap().len(), 2, "{asked}");
        }
        assert_eq!(
            sender_counts_since(&store, &since).unwrap(),
            [("billing@acme.example".to_string(), 2)]
        );
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

    #[test]
    fn each_senders_newest_few_in_the_window_come_from_one_query() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        let since = chrono::Utc::now() - chrono::Duration::minutes(100);
        // Acme writes four times, the oldest before the window; Bob once,
        // from a mixed-case address; one is the user's own sent mail.
        for (n, from, sent) in [
            (10, "billing@acme.example", false),
            (20, "Billing@Acme.example", false),
            (30, "billing@acme.example", false),
            (200, "billing@acme.example", false),
            (40, "bob@example.com", false),
            (50, "me@example.com", true),
        ] {
            let mut m = message(&dig, n, &["INBOX"]);
            m.from = Address::new(from, None);
            m.is_sent = sent;
            upsert(&store, &m).unwrap();
        }
        let recent = sender_recent_ids(&store, &since, 2).unwrap();
        let mut senders: Vec<&String> = recent.keys().collect();
        senders.sort();
        assert_eq!(senders, ["billing@acme.example", "bob@example.com"]);
        let acme: Vec<&str> = recent["billing@acme.example"]
            .iter()
            .map(|id| id.as_str())
            .collect();
        assert_eq!(
            acme,
            [
                MessageId::for_provider(&dig, "p10").as_str(),
                MessageId::for_provider(&dig, "p20").as_str()
            ]
        );
        assert_eq!(recent["bob@example.com"].len(), 1);
        assert_eq!(sender_counts_since(&store, &since).unwrap().len(), 2);
    }

    #[test]
    fn the_sift_batch_prefers_messages_with_bodies_and_takes_older_body_less_ones_only() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        // Newest first: 1 (no body), 2 (body), 3 (no body), 4 (body), 5 (no body, old).
        for (n, body) in [(1, false), (2, true), (3, false), (4, true)] {
            let mut m = message(&dig, n, &[]);
            if body {
                m.body_text = Some("hello".into());
            }
            upsert(&store, &m).unwrap();
        }
        let mut old = message(&dig, 5, &[]);
        old.date = chrono::Utc::now() - chrono::Duration::days(200);
        upsert(&store, &old).unwrap();
        let id = |n: u32| MessageId::for_provider(&dig, &format!("p{n}")).0;
        let batch = |limit: u32, before: Option<chrono::DateTime<chrono::Utc>>| -> Vec<String> {
            unclassified_ids_preferring_bodies(&store, limit, false, before)
                .unwrap()
                .into_iter()
                .map(|m| m.0)
                .collect()
        };
        // Without a body window nothing body-less is taken, however much room.
        assert_eq!(batch(10, None), [id(2), id(4)]);
        assert_eq!(batch(1, None), [id(2)]);
        // Older than the body window: the old one fills the room, the recent
        // body-less ones wait for their bodies.
        let window = chrono::Utc::now() - chrono::Duration::days(90);
        assert_eq!(batch(10, Some(window)), [id(2), id(4), id(5)]);
        assert_eq!(batch(2, Some(window)), [id(2), id(4)]);
        // A body stored as "" (the provider no longer has it) counts as landed.
        set_body(&store, &MessageId::for_provider(&dig, "p1"), Some(""), None).unwrap();
        assert_eq!(batch(10, None), [id(1), id(2), id(4)]);
    }

    #[test]
    fn labels_for_a_set_of_ids_come_from_one_query_and_land_in_one_write() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        let (a, b) = (
            message(&dig, 1, &["INBOX"]),
            message(&dig, 2, &["INBOX", "UNREAD"]),
        );
        upsert(&store, &a).unwrap();
        upsert(&store, &b).unwrap();
        let missing = MessageId::for_provider(&dig, "nope");
        let mut rows =
            labels_for_ids(&store, &[a.id.clone(), b.id.clone(), missing.clone()]).unwrap();
        rows.sort_by(|x, y| x.0.as_str().cmp(y.0.as_str()));
        assert_eq!(
            rows,
            vec![
                (a.id.clone(), a.thread_id.clone(), vec!["INBOX".to_string()]),
                (
                    b.id.clone(),
                    b.thread_id.clone(),
                    vec!["INBOX".to_string(), "UNREAD".to_string()]
                ),
            ]
        );
        let writes = store.stats().writes;
        set_labels_many(
            &store,
            &[
                (a.id.clone(), vec!["SPOIL".into()]),
                (b.id.clone(), vec!["INBOX".into()]),
            ],
        )
        .unwrap();
        assert_eq!(store.stats().writes - writes, 1, "one transaction for both");
        assert_eq!(labels_of(&store, &a.id).unwrap(), ["SPOIL"]);
        assert_eq!(labels_of(&store, &b.id).unwrap(), ["INBOX"]);
        set_labels_many(&store, &[]).unwrap();
        assert_eq!(
            store.stats().writes - writes,
            1,
            "nothing to write is no write"
        );
    }

    #[test]
    fn address_stats_for_a_set_of_senders_match_the_single_address_query() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("dig-1");
        // A newsletter with list headers and a spam verdict, a colleague the
        // user wrote to (once in To, once in Cc, once both), a phisher
        // failing DMARC, and a stranger with no mail at all.
        for n in 1..=4 {
            let mut m = message(&dig, n, &["INBOX"]);
            m.from = Address::new("news@list.example", Some("The List"));
            m.headers.list_id = Some("<list.example>".into());
            m.headers.precedence = Some("bulk".into());
            if n == 1 {
                m.labels.push("SPAM".into());
            }
            upsert(&store, &m).unwrap();
        }
        for n in 10..=12 {
            let mut m = message(&dig, n, &["INBOX"]);
            m.from = Address::new("Priya@Example.com", Some("Priya"));
            upsert(&store, &m).unwrap();
        }
        for (n, to, cc) in [(20, true, false), (21, false, true), (22, true, true)] {
            let mut m = message(&dig, n, &["SENT"]);
            m.from = Address::new("me@example.com", None);
            m.is_sent = true;
            let priya = Address::new("priya@example.com", None);
            if to {
                m.to.push(priya.clone());
            }
            if cc {
                m.cc.push(priya);
            }
            upsert(&store, &m).unwrap();
        }
        let mut phish = message(&dig, 30, &["INBOX"]);
        phish.from = Address::new("security@micros0ft.example", None);
        phish.headers.authentication.dmarc = riddle_core::AuthVerdict::Fail;
        upsert(&store, &phish).unwrap();

        let set: Vec<String> = [
            "news@list.example",
            "priya@example.com",
            "security@micros0ft.example",
            "nobody@example.com",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let many = address_stats_many(&store, &set).unwrap();
        assert_eq!(many.len(), 3, "an address with no mail is left out");
        for email in &set[..3] {
            assert_eq!(
                many[email],
                address_stats(&store, email).unwrap(),
                "{email}"
            );
        }
        let news = &many["news@list.example"];
        assert_eq!(
            (news.messages_from, news.automated, news.mailing_list),
            (4, 4, 4)
        );
        assert_eq!(news.spam_verdicts, 1);
        assert_eq!(news.name.as_deref(), Some("The List"));
        let priya = &many["priya@example.com"];
        assert_eq!((priya.messages_from, priya.messages_to), (3, 3));
        assert_eq!(many["security@micros0ft.example"].auth_failures, 1);
        let since = chrono::Utc::now() - chrono::Duration::days(1);
        let arrivals = arrivals_from_many(&store, &set, &since).unwrap();
        assert_eq!(arrivals["priya@example.com"].len(), 3);
        assert_eq!(
            arrivals["priya@example.com"],
            arrivals_from(&store, "priya@example.com", &since).unwrap()
        );
        assert!(!arrivals.contains_key("nobody@example.com"));
    }
}
