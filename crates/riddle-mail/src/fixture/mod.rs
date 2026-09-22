//! An in-memory `MailProvider` with a realistic synthetic mailbox. Tests,
//! Demo Mode and the first build (before a Gmail OAuth client exists) run
//! against it. It implements the whole trait, including label modification
//! and change history, so sync, projection and attribution are tested end to
//! end without a network: a label change made here shows up in the next
//! `history()` page exactly as Gmail would report it.
//!
//! Label ids are label names, so a test can read `Riddle/Spoil` directly.

pub mod generate;

use crate::provider::{
    Change, Detail, HistoryPage, IdPage, ListPage, MailProvider, MessageRef, OutgoingMessage,
    ProviderProfile, SentMessage, MANAGED_PREFIX,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use generate::{Generator, Overrides, Persona, Rng, Template};
use parking_lot::Mutex;
use riddle_core::{Address, DigId, Message, MessageId, ProviderKind, ProviderLabel, ThreadId};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use std::collections::{BTreeMap, HashMap};

/// How a fixture Dig is seeded; persisted in the Dig's `sync_state` so the
/// same mailbox comes back after a restart.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FixtureSpec {
    pub seed: u64,
    pub count: u32,
    pub email: String,
    pub name: String,
    pub days: u32,
}

impl Default for FixtureSpec {
    fn default() -> Self {
        Self {
            seed: 1,
            count: 400,
            email: "matt@example.com".into(),
            name: "Matt".into(),
            days: 60,
        }
    }
}

pub const SYSTEM_LABELS: &[&str] = &[
    "INBOX",
    "SENT",
    "DRAFT",
    "SPAM",
    "TRASH",
    "UNREAD",
    "STARRED",
    "IMPORTANT",
    "CATEGORY_PERSONAL",
    "CATEGORY_SOCIAL",
    "CATEGORY_PROMOTIONS",
    "CATEGORY_UPDATES",
    "CATEGORY_FORUMS",
];

struct State {
    messages: BTreeMap<String, Message>,
    labels: Vec<ProviderLabel>,
    history: Vec<(u64, Change)>,
    next_history_id: u64,
    oldest_retained: u64,
    drafts: HashMap<String, OutgoingMessage>,
    next_id: u64,
    rng: Rng,
    /// Provider calls made, for tests that assert on quota behaviour.
    calls: HashMap<&'static str, u64>,
}

pub struct FixtureProvider {
    dig: DigId,
    spec: FixtureSpec,
    persona: Persona,
    now: DateTime<Utc>,
    state: Mutex<State>,
}

/// A message with nothing in it, for tests that need one.
pub fn blank_message(dig: &str, provider_id: &str) -> Message {
    let dig = DigId::from(dig);
    Message {
        id: MessageId::for_provider(&dig, provider_id),
        dig_id: dig.clone(),
        thread_id: ThreadId::for_provider(&dig, provider_id),
        provider_id: provider_id.into(),
        subject: String::new(),
        from: Address::new("someone@example.com", None),
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: Utc::now(),
        snippet: String::new(),
        labels: vec!["INBOX".into()],
        headers: Default::default(),
        attachments: vec![],
        size_bytes: 0,
        is_sent: false,
        has_body: false,
        body_text: None,
        body_html: None,
        synced_at: Utc::now(),
    }
}

impl FixtureProvider {
    pub fn new(dig: DigId, spec: FixtureSpec) -> Self {
        Self::with_now(dig, spec, Utc::now())
    }

    /// Fixed `now` for deterministic dates in tests.
    pub fn with_now(dig: DigId, spec: FixtureSpec, now: DateTime<Utc>) -> Self {
        let persona = Persona::for_user(&spec.email, &spec.name);
        let mut generator = Generator::new(dig.clone(), &persona, spec.seed, now, 1);
        let messages = generator.mailbox(spec.count, spec.days as i64);
        let next_id = generator.next_id;
        let mut history = Vec::with_capacity(messages.len());
        let mut map = BTreeMap::new();
        for (i, m) in messages.into_iter().enumerate() {
            history.push((
                i as u64 + 1,
                Change::Added {
                    id: m.provider_id.clone(),
                },
            ));
            map.insert(m.provider_id.clone(), m);
        }
        let next_history_id = history.len() as u64 + 1;
        let labels = SYSTEM_LABELS
            .iter()
            .map(|l| ProviderLabel {
                id: (*l).into(),
                name: (*l).into(),
                managed_by_riddle: false,
                system: true,
            })
            .collect();
        rinfo!(
            Category::Sync,
            "fixture provider for dig {} seeded {} messages (seed {})",
            dig,
            map.len(),
            spec.seed
        );
        Self {
            dig,
            persona,
            now,
            state: Mutex::new(State {
                messages: map,
                labels,
                history,
                next_history_id,
                oldest_retained: 1,
                drafts: HashMap::new(),
                next_id,
                rng: Rng::new(spec.seed ^ 0xABCD),
                calls: HashMap::new(),
            }),
            spec,
        }
    }

    pub fn dig_id(&self) -> &DigId {
        &self.dig
    }
    pub fn spec(&self) -> &FixtureSpec {
        &self.spec
    }
    pub fn persona(&self) -> &Persona {
        &self.persona
    }
    /// The `now` the mailbox was generated against.
    pub fn seeded_at(&self) -> DateTime<Utc> {
        self.now
    }

    fn count(&self, name: &'static str) {
        *self.state.lock().calls.entry(name).or_insert(0) += 1;
    }
    pub fn call_count(&self, name: &str) -> u64 {
        self.state.lock().calls.get(name).copied().unwrap_or(0)
    }

    /// Every message, newest first, with bodies. For tests.
    pub fn snapshot(&self) -> Vec<Message> {
        let s = self.state.lock();
        let mut v: Vec<Message> = s.messages.values().cloned().collect();
        v.sort_by(|a, b| b.date.cmp(&a.date).then(b.provider_id.cmp(&a.provider_id)));
        v
    }

    pub fn labels_of(&self, provider_id: &str) -> Option<Vec<String>> {
        self.state
            .lock()
            .messages
            .get(provider_id)
            .map(|m| m.labels.clone())
    }

    pub fn message_count(&self) -> usize {
        self.state.lock().messages.len()
    }

    /// The current cursor, i.e. the id of the latest change.
    pub fn current_cursor(&self) -> String {
        (self.state.lock().next_history_id - 1).to_string()
    }

    /// Forget all history so any cursor but the current one is expired; the
    /// sync engine's full-resync path is exercised this way.
    pub fn expire_history(&self) {
        let mut s = self.state.lock();
        s.history.clear();
        s.oldest_retained = s.next_history_id;
    }

    fn record(s: &mut State, change: Change) {
        let id = s.next_history_id;
        s.next_history_id += 1;
        s.history.push((id, change));
    }

    /// Demo Mode: "send Riddle an email". The message appears in the next
    /// `history()` page and in listings.
    pub fn inject(
        &self,
        template: &str,
        from: Option<Address>,
        subject: Option<String>,
        body: Option<String>,
    ) -> Result<Message> {
        let template = Template::parse(template).ok_or_else(|| {
            RiddleError::invalid(format!(
                "unknown fixture template '{template}'; one of {}",
                Template::NAMES.join(", ")
            ))
        })?;
        let mut s = self.state.lock();
        let first_id = s.next_id;
        let seed = s.rng.next_u64();
        let mut g = Generator::new(self.dig.clone(), &self.persona, seed, Utc::now(), first_id);
        let overrides = Overrides {
            from,
            subject,
            body,
        };
        let mut generated = g.scenario(template, 1, &overrides);
        s.next_id = g.next_id;
        // Injected mail is new: it arrives now, in order.
        let base = Utc::now();
        let n = generated.len() as i64;
        for (i, m) in generated.iter_mut().enumerate() {
            m.date = base - chrono::Duration::seconds((n - i as i64 - 1) * 60);
            m.synced_at = base;
        }
        let last = generated
            .last()
            .cloned()
            .ok_or_else(|| RiddleError::internal("template produced nothing"))?;
        for m in generated {
            Self::record(
                &mut s,
                Change::Added {
                    id: m.provider_id.clone(),
                },
            );
            s.messages.insert(m.provider_id.clone(), m);
        }
        rinfo!(
            Category::Sync,
            "fixture dig {} injected {:?} as {}",
            self.dig,
            template,
            last.provider_id
        );
        Ok(last)
    }

    /// Add a message directly (tests that need exact content).
    pub fn insert_message(&self, m: Message) {
        let mut s = self.state.lock();
        Self::record(
            &mut s,
            Change::Added {
                id: m.provider_id.clone(),
            },
        );
        s.messages.insert(m.provider_id.clone(), m);
    }

    /// Permanently delete, as another client might; appears as `Deleted`.
    pub fn delete_message(&self, provider_id: &str) {
        let mut s = self.state.lock();
        if s.messages.remove(provider_id).is_some() {
            Self::record(
                &mut s,
                Change::Deleted {
                    id: provider_id.to_string(),
                },
            );
        }
    }

    fn strip(m: &Message, detail: Detail) -> Message {
        match detail {
            Detail::Full => m.clone(),
            Detail::Metadata => Message {
                body_text: None,
                body_html: None,
                has_body: false,
                ..m.clone()
            },
        }
    }
}

#[async_trait]
impl MailProvider for FixtureProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Fixture
    }

    async fn profile(&self) -> Result<ProviderProfile> {
        self.count("profile");
        let s = self.state.lock();
        Ok(ProviderProfile {
            email: self.persona.me.email.clone(),
            aliases: vec![format!(
                "{}+riddle@{}",
                self.persona.me.email.split('@').next().unwrap_or("me"),
                self.persona.work_domain
            )],
            name: self.persona.me.name.clone(),
            history_cursor: Some((s.next_history_id - 1).to_string()),
            messages_total: Some(s.messages.len() as u64),
        })
    }

    async fn list_labels(&self) -> Result<Vec<ProviderLabel>> {
        self.count("list_labels");
        Ok(self.state.lock().labels.clone())
    }

    async fn ensure_label(&self, name: &str) -> Result<ProviderLabel> {
        self.count("ensure_label");
        let mut s = self.state.lock();
        if let Some(l) = s.labels.iter().find(|l| l.name == name) {
            return Ok(l.clone());
        }
        let label = ProviderLabel {
            id: name.to_string(),
            name: name.to_string(),
            managed_by_riddle: name.starts_with(MANAGED_PREFIX),
            system: false,
        };
        s.labels.push(label.clone());
        Ok(label)
    }

    async fn list_message_ids(&self, page: &ListPage) -> Result<IdPage> {
        self.count("list");
        let s = self.state.lock();
        let mut all: Vec<&Message> = s
            .messages
            .values()
            .filter(|m| {
                !m.labels.iter().any(|l| l == "TRASH" || l == "SPAM")
                    || page
                        .label
                        .as_deref()
                        .is_some_and(|l| l == "TRASH" || l == "SPAM")
            })
            .filter(|m| page.after.is_none_or(|a| m.date >= a))
            .filter(|m| page.before.is_none_or(|b| m.date < b))
            .filter(|m| page.label.as_ref().is_none_or(|l| m.labels.contains(l)))
            .collect();
        all.sort_by(|a, b| b.date.cmp(&a.date).then(b.provider_id.cmp(&a.provider_id)));
        let offset: usize = page
            .page_token
            .as_deref()
            .and_then(|t| t.parse().ok())
            .unwrap_or(0);
        let max = page.max.clamp(1, 500) as usize;
        let refs: Vec<MessageRef> = all
            .iter()
            .skip(offset)
            .take(max)
            .map(|m| MessageRef {
                provider_id: m.provider_id.clone(),
                thread_provider_id: m.thread_id.provider_id().to_string(),
            })
            .collect();
        let next = if offset + max < all.len() {
            Some((offset + max).to_string())
        } else {
            None
        };
        Ok(IdPage {
            refs,
            next_page_token: next,
            result_size_estimate: Some(all.len() as u64),
        })
    }

    async fn get_message(&self, provider_id: &str, detail: Detail) -> Result<Message> {
        self.count("get");
        let s = self.state.lock();
        s.messages
            .get(provider_id)
            .map(|m| Self::strip(m, detail))
            .ok_or_else(|| RiddleError::not_found(format!("no message {provider_id}")))
    }

    async fn get_messages_batch(&self, ids: &[String], detail: Detail) -> Result<Vec<Message>> {
        self.count("get_batch");
        let s = self.state.lock();
        Ok(ids
            .iter()
            .filter_map(|id| s.messages.get(id).map(|m| Self::strip(m, detail)))
            .collect())
    }

    async fn history(&self, cursor: &str) -> Result<HistoryPage> {
        self.count("history");
        let s = self.state.lock();
        let c: u64 = cursor
            .trim()
            .parse()
            .map_err(|_| RiddleError::invalid(format!("bad fixture cursor '{cursor}'")))?;
        if c + 1 < s.oldest_retained {
            return Ok(HistoryPage {
                changes: vec![],
                next_cursor: None,
                expired: true,
            });
        }
        let changes: Vec<Change> = s
            .history
            .iter()
            .filter(|(id, _)| *id > c)
            .map(|(_, ch)| ch.clone())
            .collect();
        Ok(HistoryPage {
            changes,
            next_cursor: Some((s.next_history_id - 1).to_string()),
            expired: false,
        })
    }

    async fn modify_labels(&self, ids: &[String], add: &[String], remove: &[String]) -> Result<()> {
        self.count("modify_labels");
        let mut s = self.state.lock();
        for a in add {
            if !s.labels.iter().any(|l| &l.id == a) {
                return Err(RiddleError::invalid(format!("unknown label id {a}")));
            }
        }
        let mut changes = Vec::new();
        for id in ids {
            let Some(m) = s.messages.get_mut(id) else {
                continue;
            };
            let mut added = Vec::new();
            let mut removed = Vec::new();
            for a in add {
                if !m.labels.contains(a) {
                    m.labels.push(a.clone());
                    added.push(a.clone());
                }
            }
            for r in remove {
                if let Some(pos) = m.labels.iter().position(|l| l == r) {
                    m.labels.remove(pos);
                    removed.push(r.clone());
                }
            }
            if !added.is_empty() || !removed.is_empty() {
                changes.push(Change::LabelsChanged {
                    id: id.clone(),
                    added,
                    removed,
                });
            }
        }
        for ch in changes {
            Self::record(&mut s, ch);
        }
        Ok(())
    }

    async fn create_draft(&self, message: &OutgoingMessage) -> Result<String> {
        self.count("create_draft");
        let mut s = self.state.lock();
        let id = format!("draft{:04x}", s.next_id);
        s.next_id += 1;
        s.drafts.insert(id.clone(), message.clone());
        Ok(id)
    }

    async fn update_draft(
        &self,
        provider_draft_id: &str,
        message: &OutgoingMessage,
    ) -> Result<String> {
        self.count("update_draft");
        let mut s = self.state.lock();
        if !s.drafts.contains_key(provider_draft_id) {
            return Err(RiddleError::not_found(format!(
                "no draft {provider_draft_id}"
            )));
        }
        s.drafts
            .insert(provider_draft_id.to_string(), message.clone());
        Ok(provider_draft_id.to_string())
    }

    async fn delete_draft(&self, provider_draft_id: &str) -> Result<()> {
        self.count("delete_draft");
        self.state.lock().drafts.remove(provider_draft_id);
        Ok(())
    }

    async fn send_draft(&self, provider_draft_id: &str) -> Result<SentMessage> {
        let m = self
            .state
            .lock()
            .drafts
            .remove(provider_draft_id)
            .ok_or_else(|| RiddleError::not_found(format!("no draft {provider_draft_id}")))?;
        self.send(&m).await
    }

    async fn send(&self, message: &OutgoingMessage) -> Result<SentMessage> {
        self.count("send");
        if message.to.is_empty() && message.cc.is_empty() && message.bcc.is_empty() {
            return Err(RiddleError::invalid(
                "a message needs at least one recipient",
            ));
        }
        let mut s = self.state.lock();
        let first_id = s.next_id;
        let mut g = Generator::new(
            self.dig.clone(),
            &self.persona,
            s.rng.next_u64(),
            Utc::now(),
            first_id,
        );
        let thread = message
            .thread_provider_id
            .as_ref()
            .map(|t| ThreadId::for_provider(&self.dig, t));
        let mut m = g.blank(thread.as_ref(), Utc::now());
        s.next_id = g.next_id;
        m.from = Address::new(
            self.persona.me.email.clone(),
            self.persona.me.name.as_deref(),
        );
        m.to = message.to.clone();
        m.cc = message.cc.clone();
        m.bcc = message.bcc.clone();
        m.subject = message.subject.clone();
        m.is_sent = true;
        m.labels = vec!["SENT".into()];
        m.headers.in_reply_to = message.in_reply_to.clone();
        m.headers.references = message.references.clone();
        m.headers.message_id = Some(crate::mime::new_message_id(&self.persona.me.email));
        m.body_text = Some(message.body_text.clone());
        m.body_html = message.body_html.clone();
        m.snippet = message.body_text.chars().take(120).collect();
        m.size_bytes = (message.body_text.len() + 600) as u64;
        let sent = SentMessage {
            provider_id: m.provider_id.clone(),
            thread_provider_id: m.thread_id.provider_id().to_string(),
        };
        Self::record(
            &mut s,
            Change::Added {
                id: m.provider_id.clone(),
            },
        );
        s.messages.insert(m.provider_id.clone(), m);
        Ok(sent)
    }

    async fn trash(&self, ids: &[String]) -> Result<()> {
        self.modify_labels(ids, &["TRASH".into()], &["INBOX".into()])
            .await
    }

    async fn search(&self, query: &str, max: u32) -> Result<Vec<MessageRef>> {
        let q = query.to_lowercase();
        let s = self.state.lock();
        let mut hits: Vec<&Message> = s
            .messages
            .values()
            .filter(|m| {
                m.subject.to_lowercase().contains(&q)
                    || m.from.email.contains(&q)
                    || m.body_text
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains(&q))
            })
            .collect();
        hits.sort_by_key(|m| std::cmp::Reverse(m.date));
        Ok(hits
            .into_iter()
            .take(max as usize)
            .map(|m| MessageRef {
                provider_id: m.provider_id.clone(),
                thread_provider_id: m.thread_id.provider_id().to_string(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> FixtureProvider {
        FixtureProvider::new(
            DigId::from("d1"),
            FixtureSpec {
                count: 150,
                seed: 3,
                ..Default::default()
            },
        )
    }

    #[tokio::test]
    async fn listing_is_newest_first_and_pages_to_the_end() {
        let p = provider();
        let mut token = None;
        let mut seen = 0;
        let mut last_date: Option<DateTime<Utc>> = None;
        loop {
            let page = p
                .list_message_ids(&ListPage {
                    max: 40,
                    page_token: token.clone(),
                    ..Default::default()
                })
                .await
                .unwrap();
            for r in &page.refs {
                let m = p
                    .get_message(&r.provider_id, Detail::Metadata)
                    .await
                    .unwrap();
                assert!(!m.has_body);
                if let Some(d) = last_date {
                    assert!(m.date <= d);
                }
                last_date = Some(m.date);
                seen += 1;
            }
            token = page.next_page_token;
            if token.is_none() {
                break;
            }
        }
        let in_spam = p
            .snapshot()
            .iter()
            .filter(|m| m.labels.contains(&"SPAM".to_string()))
            .count();
        assert_eq!(seen + in_spam, p.message_count());
    }

    #[tokio::test]
    async fn label_changes_and_injections_appear_in_history_after_the_cursor() {
        let p = provider();
        let cursor = p.profile().await.unwrap().history_cursor.unwrap();
        let empty = p.history(&cursor).await.unwrap();
        assert!(empty.changes.is_empty() && !empty.expired);
        let spoil = p.ensure_label("Riddle/Spoil").await.unwrap();
        assert!(spoil.managed_by_riddle);
        let first = p
            .snapshot()
            .iter()
            .find(|m| m.labels.iter().any(|l| l == "INBOX"))
            .unwrap()
            .provider_id
            .clone();
        p.modify_labels(
            std::slice::from_ref(&first),
            std::slice::from_ref(&spoil.id),
            &["INBOX".into()],
        )
        .await
        .unwrap();
        let injected = p
            .inject("personal_question", None, Some("Ping".into()), None)
            .unwrap();
        let page = p.history(&cursor).await.unwrap();
        assert_eq!(page.changes.len(), 2);
        assert_eq!(
            page.changes[0],
            Change::LabelsChanged {
                id: first.clone(),
                added: vec!["Riddle/Spoil".into()],
                removed: vec!["INBOX".into()]
            }
        );
        assert_eq!(
            page.changes[1],
            Change::Added {
                id: injected.provider_id.clone()
            }
        );
        assert_eq!(injected.subject, "Ping");
        let again = p
            .history(page.next_cursor.as_deref().unwrap())
            .await
            .unwrap();
        assert!(again.changes.is_empty());
        p.expire_history();
        assert!(p.history(&cursor).await.unwrap().expired);
        assert!(!p.history(&p.current_cursor()).await.unwrap().expired);
    }

    #[tokio::test]
    async fn sending_creates_a_sent_message_in_the_thread() {
        let p = provider();
        let target = p.snapshot().into_iter().find(|m| !m.is_sent).unwrap();
        let out = OutgoingMessage {
            to: vec![target.from.clone()],
            subject: format!("Re: {}", target.subject),
            body_text: "Thanks!".into(),
            thread_provider_id: Some(target.thread_id.provider_id().to_string()),
            in_reply_to: target.headers.message_id.clone(),
            ..Default::default()
        };
        let sent = p.send(&out).await.unwrap();
        assert_eq!(sent.thread_provider_id, target.thread_id.provider_id());
        let m = p
            .get_message(&sent.provider_id, Detail::Full)
            .await
            .unwrap();
        assert!(m.is_sent && m.labels == vec!["SENT".to_string()]);
        assert_eq!(m.headers.in_reply_to, target.headers.message_id);
        let err = p.send(&OutgoingMessage::default()).await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
    }
}
