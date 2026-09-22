//! Exact search with Gmail operator parity, across Digs (`ARCHITECTURE.md`
//! §45). A query is parsed into a `MessageFilter` plus `SearchExtras` and
//! an FTS5 expression; each Dig store answers, and the pages are merged by
//! date. Operators the store cannot honour are reported in `unsupported`
//! rather than silently dropped, so the UI can say so.
//!
//! Supported: `from:`, `to:`, `subject:`, `has:attachment`, `before:`,
//! `after:` (YYYY/MM/DD or YYYY-MM-DD), `newer_than:7d|2m|1y`,
//! `older_than:`, `label:`, `in:inbox|sent|spoil|finds|unresolved|
//! contamination|filed`, `is:sent|unread|starred|important`, `stratum:`,
//! `thread:`, quoted phrases, `-term` negation, free text.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use riddle_core::{Dig, DigId, MessageSummary, Page, PageRequest};
use riddle_errors::Result;
use riddle_observability::{rdebug, Category};
use riddle_store::repo::messages::{MessageFilter, SearchExtras};
use riddle_store::{repo, Stores};

#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    pub filter: MessageFilter,
    pub extras: SearchExtras,
    /// Free-text terms (already folded into `extras.fts`).
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub negated: Vec<String>,
    pub unsupported: Vec<String>,
}

fn tokens(q: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in q.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y/%m/%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .ok()
}

fn relative(s: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let (num, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: i64 = num.parse().ok()?;
    Some(match unit {
        "d" => now - Duration::days(n),
        "m" => now - Duration::days(30 * n),
        "y" => now - Duration::days(365 * n),
        "h" => now - Duration::hours(n),
        _ => return None,
    })
}

fn fts_term(t: &str) -> String {
    format!("\"{}\"", t.replace('"', ""))
}

/// Parse a Gmail-style query.
pub fn parse(q: &str, now: DateTime<Utc>) -> ParsedQuery {
    let mut p = ParsedQuery::default();
    let mut fts: Vec<String> = Vec::new();
    let mut nots: Vec<String> = Vec::new();
    let all = tokens(q);
    let n = all.len();
    for (i, raw) in all.into_iter().enumerate() {
        let last = i + 1 == n;
        let (neg, tok) = match raw.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest.to_string()),
            _ => (false, raw.clone()),
        };
        if tok.starts_with('"') {
            let phrase = tok.trim_matches('"').to_string();
            if phrase.is_empty() {
                continue;
            }
            if neg {
                nots.push(fts_term(&phrase));
                p.negated.push(phrase);
            } else {
                fts.push(fts_term(&phrase));
                p.phrases.push(phrase);
            }
            continue;
        }
        let Some((op, val)) = tok.split_once(':').filter(|(op, v)| {
            !op.is_empty()
                && !v.is_empty()
                && op.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        }) else {
            if neg {
                nots.push(fts_term(&tok));
                p.negated.push(tok);
            } else {
                // Prefix-match the last term so typing feels live.
                fts.push(if last {
                    format!("{}*", fts_term(&tok))
                } else {
                    fts_term(&tok)
                });
                p.terms.push(tok);
            }
            continue;
        };
        let val = val.trim_matches('"').to_string();
        match op.to_ascii_lowercase().as_str() {
            "from" => {
                if neg {
                    p.extras.exclude_from.push(val);
                } else {
                    p.filter.from = Some(val);
                }
            }
            "to" | "cc" => p.extras.to = Some(val),
            "subject" => fts.push(format!("subject:{}", fts_term(&val))),
            "has" if val.eq_ignore_ascii_case("attachment") => p.extras.has_attachment = Some(!neg),
            "before" => match parse_date(&val) {
                Some(d) => {
                    p.filter.until =
                        Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap_or_default()))
                }
                None => p.unsupported.push(tok.clone()),
            },
            "after" => match parse_date(&val) {
                Some(d) => {
                    p.filter.since =
                        Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap_or_default()))
                }
                None => p.unsupported.push(tok.clone()),
            },
            "newer_than" => match relative(&val, now) {
                Some(t) => p.filter.since = Some(t),
                None => p.unsupported.push(tok.clone()),
            },
            "older_than" => match relative(&val, now) {
                Some(t) => p.filter.until = Some(t),
                None => p.unsupported.push(tok.clone()),
            },
            "label" => p.filter.label = Some(val),
            "stratum" => p.filter.stratum = Some(val),
            "thread" => p.filter.thread_id = Some(val),
            "in" => match val.to_ascii_lowercase().as_str() {
                "inbox" => p.filter.label = Some("INBOX".into()),
                "sent" => p.extras.is_sent = Some(true),
                "spam" => p.filter.label = Some("SPAM".into()),
                "trash" => p.filter.label = Some("TRASH".into()),
                "anywhere" | "all" => {}
                "spoil" | "unresolved" | "contamination" | "filed" => {
                    p.extras.disposition_in.push(val.to_ascii_lowercase())
                }
                "finds" | "find" => p.extras.disposition_in.push("find".into()),
                _ => p.unsupported.push(tok.clone()),
            },
            "is" => match val.to_ascii_lowercase().as_str() {
                "sent" => p.extras.is_sent = Some(!neg),
                "unread" => p.filter.label = Some("UNREAD".into()),
                "starred" => p.filter.label = Some("STARRED".into()),
                "important" => p.filter.label = Some("IMPORTANT".into()),
                "unclassified" => p.filter.unclassified_only = true,
                _ => p.unsupported.push(tok.clone()),
            },
            "find" | "type" => p.filter.find_type = Some(val.to_ascii_lowercase()),
            "filename" => {
                p.extras.has_attachment = Some(true);
                fts.push(fts_term(&val));
            }
            _ => p.unsupported.push(tok.clone()),
        }
    }
    let mut expr = fts.join(" ");
    if !nots.is_empty() {
        if expr.is_empty() {
            // FTS5 needs a positive side; match everything on the sender column.
            expr = "from_email:\"@\"*".into();
        }
        expr = format!("({expr}) NOT ({})", nots.join(" OR "));
    }
    if !expr.is_empty() {
        p.extras.fts = Some(expr);
    }
    p
}

/// Search one Dig.
pub fn search_dig(
    stores: &Stores,
    dig: &DigId,
    parsed: &ParsedQuery,
    page: &PageRequest,
) -> Result<Page<MessageSummary>> {
    let store = stores.dig(dig)?;
    repo::messages::search(&store, dig, &parsed.filter, &parsed.extras, page)
}

/// Search across Digs, merging by date. Paging is by offset over the merged
/// list, so each Dig is asked for `offset + limit` rows.
pub fn search(
    stores: &Stores,
    digs: &[Dig],
    query: &str,
    page: &PageRequest,
    now: DateTime<Utc>,
) -> Result<Page<MessageSummary>> {
    let parsed = parse(query, now);
    rdebug!(
        Category::Store,
        "search across {} digs: {} terms, {} operators unsupported",
        digs.len(),
        parsed.terms.len(),
        parsed.unsupported.len()
    );
    let limit = page.limit();
    let offset = page.offset();
    if digs.len() == 1 {
        return search_dig(stores, &digs[0].id, &parsed, page);
    }
    let wide = PageRequest {
        cursor: None,
        limit: Some((offset + limit as u64).min(500) as u32),
    };
    let mut all: Vec<MessageSummary> = Vec::new();
    let mut total = 0u64;
    for d in digs {
        let p = search_dig(stores, &d.id, &parsed, &wide)?;
        total += p.total.unwrap_or(p.items.len() as u64);
        all.extend(p.items);
    }
    all.sort_by_key(|m| std::cmp::Reverse(m.date));
    let items: Vec<MessageSummary> = all
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    Ok(Page::from_offset(items, offset, limit, Some(total)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{DigStatus, PrivacyMode, ProjectionMode, ProviderKind};
    use riddle_mail::{FixtureProvider, FixtureSpec};

    #[test]
    fn operators_map_to_filters_and_free_text_to_fts() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0).unwrap();
        let p = parse("from:priya to:matt subject:numbers has:attachment before:2026/09/20 after:2026-09-01 label:INBOX in:spoil stratum:Finance is:sent \"board deck\" -receipt quarterly", now);
        assert_eq!(p.filter.from.as_deref(), Some("priya"));
        assert_eq!(p.extras.to.as_deref(), Some("matt"));
        assert_eq!(p.extras.has_attachment, Some(true));
        assert_eq!(
            p.filter.until.unwrap().format("%Y-%m-%d").to_string(),
            "2026-09-20"
        );
        assert_eq!(
            p.filter.since.unwrap().format("%Y-%m-%d").to_string(),
            "2026-09-01"
        );
        assert_eq!(p.filter.label.as_deref(), Some("INBOX"));
        assert_eq!(p.extras.disposition_in, vec!["spoil".to_string()]);
        assert_eq!(p.filter.stratum.as_deref(), Some("Finance"));
        assert_eq!(p.extras.is_sent, Some(true));
        assert_eq!(p.phrases, vec!["board deck".to_string()]);
        assert_eq!(p.negated, vec!["receipt".to_string()]);
        assert_eq!(p.terms, vec!["quarterly".to_string()]);
        assert_eq!(
            p.extras.fts.as_deref(),
            Some("(subject:\"numbers\" \"board deck\" \"quarterly\"*) NOT (\"receipt\")")
        );
        assert!(p.unsupported.is_empty());
    }

    #[test]
    fn relative_dates_and_unknown_operators_are_handled() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0).unwrap();
        let p = parse(
            "newer_than:7d older_than:1m in:finds -from:github.com flavour:odd",
            now,
        );
        assert_eq!(p.filter.since, Some(now - Duration::days(7)));
        assert_eq!(p.filter.until, Some(now - Duration::days(30)));
        assert_eq!(p.extras.disposition_in, vec!["find".to_string()]);
        assert_eq!(p.extras.exclude_from, vec!["github.com".to_string()]);
        assert_eq!(p.unsupported, vec!["flavour:odd".to_string()]);
        assert!(p.extras.fts.is_none());
        let only_neg = parse("-newsletter", now);
        assert!(only_neg.extras.fts.as_deref().unwrap().contains("NOT"));
    }

    fn seeded() -> (Stores, Vec<Dig>) {
        let stores = Stores::in_memory().unwrap();
        let mut digs = Vec::new();
        for (i, seed) in [(0, 3u64), (1, 4u64)] {
            let dig = Dig {
                id: DigId::from(format!("d{i}")),
                name: format!("Dig {i}"),
                email: "matt@example.com".into(),
                provider: ProviderKind::Fixture,
                group: None,
                privacy: PrivacyMode::CloudJudgment,
                projection: ProjectionMode::Full,
                status: DigStatus::NotConnected,
                shadow_started_at: None,
                created_at: Utc::now(),
            };
            let fx = FixtureProvider::new(
                dig.id.clone(),
                FixtureSpec {
                    count: 80,
                    seed,
                    ..Default::default()
                },
            );
            let store = stores.dig(&dig.id).unwrap();
            let all = fx.snapshot();
            store
                .write(|c| {
                    for m in &all {
                        repo::messages::upsert_in(c, m)?;
                    }
                    Ok(())
                })
                .unwrap();
            digs.push(dig);
        }
        (stores, digs)
    }

    #[test]
    fn a_search_across_digs_merges_by_date_and_honours_operators() {
        let (stores, digs) = seeded();
        let page = search(
            &stores,
            &digs,
            "from:github.com",
            &PageRequest {
                cursor: None,
                limit: Some(20),
            },
            Utc::now(),
        )
        .unwrap();
        assert!(!page.items.is_empty());
        assert!(page
            .items
            .iter()
            .all(|m| m.from.email.ends_with("github.com")));
        assert!(page.items.windows(2).all(|w| w[0].date >= w[1].date));
        let both: std::collections::HashSet<_> =
            page.items.iter().map(|m| m.dig_id.clone()).collect();
        assert_eq!(both.len(), 2, "results come from both Digs");
        let sent = search(
            &stores,
            &digs,
            "is:sent",
            &PageRequest::default(),
            Utc::now(),
        )
        .unwrap();
        assert!(sent.items.iter().all(|m| m.is_sent));
        let attach = search(
            &stores,
            &digs,
            "has:attachment invoice",
            &PageRequest::default(),
            Utc::now(),
        )
        .unwrap();
        assert!(!attach.items.is_empty());
        assert!(attach.items.iter().all(|m| m.has_attachments));
        let phrase = search(
            &stores,
            &digs,
            "\"review requested\"",
            &PageRequest::default(),
            Utc::now(),
        )
        .unwrap();
        assert!(phrase.items.iter().all(|m| m.subject.contains("PR #")));
        let none = search(
            &stores,
            &digs,
            "from:nobody@nowhere.example",
            &PageRequest::default(),
            Utc::now(),
        )
        .unwrap();
        assert!(none.items.is_empty());
        let paged = search(
            &stores,
            &digs,
            "from:github.com",
            &PageRequest {
                cursor: Some("5".into()),
                limit: Some(5),
            },
            Utc::now(),
        )
        .unwrap();
        assert_eq!(paged.items.len(), 5);
        assert_eq!(paged.items[0].id, page.items[5].id);
    }
}
