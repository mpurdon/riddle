//! The per-Dig sync engine (`ARCHITECTURE.md` §53, §15).
//!
//! Initial sync lists newest first and fetches metadata only, one store
//! transaction per page, checkpointing the page token in `sync_state` so a
//! restart resumes rather than restarts. It lists `pages_per_run` pages a
//! call and returns `complete: false` with the token kept, so the job that
//! runs it can yield and the queue can sift and fetch bodies for what has
//! landed between runs: it used to list the whole window in one job, and
//! with a large mailbox nothing was sorted until the listing finished,
//! thirty-five minutes for ten thousand messages (17 Oct 2026). Bodies
//! come later through `fetch_bodies`, newest first, because Sift's facts
//! need headers and the summary needs to be honest quickly. After the
//! window, `backfill_history` lists older mail in the same bounded runs
//! with each page's bodies, down to the mailbox start or the Dig's
//! `history_days`. Incremental sync follows the history cursor; when the
//! provider says the cursor is gone, a full resync lists again and
//! reconciles labels against the expected projection, reporting
//! differences and never overwriting.
//!
//! People are refreshed once per run for the addresses the run touched and
//! rebuilt whole when a listing finishes, not per page: every page used to
//! refresh each of its addresses with seven queries apiece. The label
//! catalogue is rewritten only when its listing changed, and a page of
//! label changes is one read and one write.
//!
//! Attribution: a provider-side label change that matches a projection row
//! applied recently is Riddle's own write echoing back and is ignored.
//! Anything else is external and is surfaced as an `ExternalMove` for the
//! correction layer (§46 `external_move`); only a human counts as a
//! correction, and that decision is the integrator's, made with the Dig's
//! filters in view. Sync never decides it.

use chrono::{DateTime, Duration, Utc};
use riddle_core::{Dig, DigId, EventSink, Message, MessageId, RiddleEvent, ThreadId, UserProfile};
use riddle_errors::{ErrorKind, Result};
use riddle_mail::{Change, Detail, ListPage, MailProvider};
use riddle_observability::{rdebug, rinfo, rwarn, swallowed, Category, Timed};
use riddle_projection::plan::is_volatile as is_volatile_label;
use riddle_projection::{LabelResolver, ProjectionOp};
use riddle_store::{repo, Store, Stores};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

pub const INITIAL_PAGE_TOKEN: &str = "initial_page_token";
pub const INITIAL_LISTED: &str = "initial_listed";
/// The provider's estimate of the window's size, kept so a resumed listing
/// shows done/total from its first page.
pub const INITIAL_TOTAL: &str = "initial_total";
pub const INITIAL_STARTED_AT: &str = "initial_started_at";
/// Where the first sync's window started, fixed on its first run: the
/// listing resumes over the same span after a restart a day later, and the
/// backfill lists before it whatever the window setting says since.
pub const INITIAL_WINDOW_START: &str = "initial_window_start";
pub const RESYNC_REQUIRED: &str = "resync_required";
pub const BACKFILL_PAGE_TOKEN: &str = "backfill_page_token";
pub const BACKFILL_LISTED: &str = "backfill_listed";
pub const BACKFILL_TOTAL: &str = "backfill_total";
/// The oldest message the backfill has fetched, for "through March 2024".
pub const BACKFILL_OLDEST: &str = "backfill_oldest";
/// The floor the backfill lists down to, fixed on its first run: a page
/// token belongs to one query, so the query must not move between runs.
/// `none` is the mailbox start.
pub const BACKFILL_FLOOR: &str = "backfill_floor";
pub const BACKFILL_DONE: &str = "backfill_done";
/// Set when a listing lands metadata whose bodies are still to fetch, and
/// cleared when `fetch_bodies` finds nothing left. While it is set, Sift
/// takes messages whose bodies have landed first; a message that reaches
/// the store some other way (a test, a tool) is judged on what it has.
pub const BODIES_PENDING: &str = "bodies_pending";

#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub page_size: u32,
    /// How far back the initial sync and a resync list; `None` is everything.
    pub window_days: Option<i64>,
    pub body_batch: u32,
    /// How long after a projection applied its echo is still expected.
    pub echo_window: Duration,
    /// Pages the initial sync or the backfill lists in one call before
    /// returning `complete: false` with its token kept; a thousand
    /// messages is about a minute on Gmail. 0 lists to the end.
    pub pages_per_run: u32,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            page_size: 100,
            window_days: Some(90),
            body_batch: 50,
            echo_window: Duration::minutes(30),
            pages_per_run: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Listing,
    Metadata,
    Bodies,
    Incremental,
    Resync,
    /// Older mail than the window, after the first sync (§53).
    Backfill,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProgress {
    pub dig_id: DigId,
    pub phase: SyncPhase,
    pub done: u64,
    pub total: Option<u64>,
    pub step: String,
}

/// A label change made by something other than Riddle's projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMove {
    pub dig_id: DigId,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub at: DateTime<Utc>,
}

/// Provider labels disagree with Riddle's expected projection (§15 resync).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Difference {
    pub message_id: MessageId,
    pub expected_add_missing: Vec<String>,
    pub expected_remove_present: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncKind {
    Initial,
    Incremental,
    Resync,
    Backfill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub kind: SyncKind,
    pub added: u64,
    pub updated: u64,
    pub deleted: u64,
    pub label_changes: u64,
    pub echoes: u64,
    pub external_moves: Vec<ExternalMove>,
    pub differences: Vec<Difference>,
    pub cursor: Option<String>,
    /// False when cancelled, or when a listing (initial or backfill) has
    /// pages left after this run: the caller runs it again.
    pub complete: bool,
    /// How far a listing has come over all its runs, and the provider's
    /// estimate of its size, for the job's checkpoint.
    pub listed: u64,
    pub total: Option<u64>,
}

impl SyncReport {
    fn new(kind: SyncKind) -> Self {
        Self {
            kind,
            added: 0,
            updated: 0,
            deleted: 0,
            label_changes: 0,
            echoes: 0,
            external_moves: vec![],
            differences: vec![],
            cursor: None,
            complete: true,
            listed: 0,
            total: None,
        }
    }
}

/// Text from HTML when the provider sent none, so FTS and the judge have
/// something to read.
fn with_body_text(mut m: Message) -> Message {
    if m.body_text.is_none() {
        m.body_text = Some(
            m.body_html
                .as_deref()
                .map(riddle_mail::normalize::html_to_text)
                .unwrap_or_default(),
        );
    }
    m
}

fn parse_time(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc))
}

fn parse_count(s: Option<String>) -> Option<u64> {
    s.and_then(|s| s.parse().ok())
}

/// The first sync's window start, recorded on its first run. For a Dig
/// synced before that was recorded (`initial_sync_done` and no start: every
/// account from before this build), the oldest message it holds, recorded
/// now, so an existing account backfills exactly as a new one would; on
/// the first merged build no such account armed a backfill (17 Oct 2026).
/// `None` while the first listing is not done, or for an empty Dig.
pub fn window_start_or_derived(store: &Store) -> Result<Option<DateTime<Utc>>> {
    if let Some(t) = parse_time(repo::sync_state::get(store, INITIAL_WINDOW_START)?) {
        return Ok(Some(t));
    }
    if repo::sync_state::get(store, repo::sync_state::INITIAL_SYNC_DONE)?.as_deref() != Some("1") {
        return Ok(None);
    }
    let Some(oldest) = repo::messages::oldest_date(store)? else {
        return Ok(None);
    };
    repo::sync_state::set(store, INITIAL_WINDOW_START, &oldest.to_rfc3339())?;
    Ok(Some(oldest))
}

/// A cancellation flag with an async wait, for `tokio::select!`.
#[derive(Clone, Default)]
pub struct CancelFlag {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
    pub async fn wait(&self) {
        while !self.is_cancelled() {
            self.notify.notified().await;
        }
    }
}

pub type ProgressFn<'a> = &'a mut (dyn FnMut(SyncProgress) + Send);
pub type ExternalFn<'a> = &'a mut (dyn FnMut(ExternalMove) + Send);

pub struct SyncEngine {
    dig: Dig,
    provider: Arc<dyn MailProvider>,
    stores: Arc<Stores>,
    events: Arc<dyn EventSink>,
    options: SyncOptions,
    resolver: tokio::sync::Mutex<LabelResolver>,
}

impl SyncEngine {
    pub fn new(
        dig: Dig,
        provider: Arc<dyn MailProvider>,
        stores: Arc<Stores>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            dig,
            provider,
            stores,
            events,
            options: SyncOptions::default(),
            resolver: tokio::sync::Mutex::new(LabelResolver::default()),
        }
    }
    pub fn with_options(mut self, options: SyncOptions) -> Self {
        self.options = options;
        self
    }
    pub fn dig(&self) -> &Dig {
        &self.dig
    }
    pub fn options(&self) -> &SyncOptions {
        &self.options
    }

    fn store(&self) -> Result<Arc<Store>> {
        self.stores.dig(&self.dig.id)
    }
    pub fn dig_store(&self) -> Result<Arc<Store>> {
        self.store()
    }
    fn profile(&self) -> Result<UserProfile> {
        let mut p = repo::user_profile::get(&self.stores.index())?;
        if !p.is_me(&self.dig.email) {
            p.addresses.push(self.dig.email.clone());
        }
        Ok(p)
    }
    fn window_start(&self) -> Option<DateTime<Utc>> {
        self.options
            .window_days
            .map(|d| Utc::now() - Duration::days(d))
    }

    pub fn initial_sync_done(&self) -> Result<bool> {
        let store = self.store()?;
        Ok(
            repo::sync_state::get(&store, repo::sync_state::INITIAL_SYNC_DONE)?.as_deref()
                == Some("1"),
        )
    }

    /// The account's label names, kept for the app to show. One provider call
    /// a sync, and one write only when the listing changed: the catalogue
    /// was deleted and reinserted on every poll. Bookkeeping, so a failure
    /// is logged and the sync goes on.
    async fn refresh_labels(&self, store: &Store) {
        let result = async {
            let labels = self.provider.list_labels().await?;
            if repo::provider_labels::holds(store, &labels)? {
                return Ok(());
            }
            repo::provider_labels::replace_all(store, &labels)
        }
        .await;
        if let Err(e) = result {
            swallowed!(
                Category::Sync,
                "label_list_refresh",
                e,
                "label list for {} not refreshed",
                self.dig.id
            );
        }
    }

    /// Whether the first listing is done but the history behind its window
    /// has not been fetched to the floor yet: the backfill job is due.
    pub fn backfill_pending(&self) -> Result<bool> {
        let store = self.store()?;
        Ok(self.initial_sync_done()?
            && repo::sync_state::get(&store, BACKFILL_DONE)?.as_deref() != Some("1")
            && window_start_or_derived(&store)?.is_some())
    }

    /// Initial if not done, else incremental; a resync when the cursor expired.
    pub async fn sync_once(
        &self,
        progress: ProgressFn<'_>,
        cancel: &CancelFlag,
        on_external: ExternalFn<'_>,
    ) -> Result<SyncReport> {
        let store = self.store()?;
        self.refresh_labels(&store).await;
        if repo::sync_state::get(&store, RESYNC_REQUIRED)?.is_some() {
            return self.full_resync(progress, cancel).await;
        }
        if !self.initial_sync_done()? {
            return self.initial_sync(progress, cancel).await;
        }
        self.incremental(progress, cancel, on_external).await
    }

    /// Newest first, metadata only, one transaction per page, resumable:
    /// `pages_per_run` pages a call, then `complete: false` with the token
    /// kept for the next call, which may come after a restart.
    pub async fn initial_sync(
        &self,
        progress: ProgressFn<'_>,
        cancel: &CancelFlag,
    ) -> Result<SyncReport> {
        let _t = Timed::start(
            Category::Sync,
            format!("initial sync {}", self.dig.id),
            std::time::Duration::from_secs(600),
        );
        let store = self.store()?;
        let mut report = SyncReport::new(SyncKind::Initial);
        // Record the cursor before listing so nothing arriving mid-listing is lost.
        if repo::sync_state::get(&store, repo::sync_state::HISTORY_CURSOR)?.is_none() {
            let profile = self.provider.profile().await?;
            if let Some(c) = profile.history_cursor {
                repo::sync_state::set(&store, repo::sync_state::HISTORY_CURSOR, &c)?;
            }
            repo::sync_state::set(&store, INITIAL_STARTED_AT, &Utc::now().to_rfc3339())?;
        }
        let mut page_token = repo::sync_state::get(&store, INITIAL_PAGE_TOKEN)?;
        let mut listed: u64 =
            parse_count(repo::sync_state::get(&store, INITIAL_LISTED)?).unwrap_or(0);
        let mut total: Option<u64> = parse_count(repo::sync_state::get(&store, INITIAL_TOTAL)?);
        // The window start is fixed on the first run (see `INITIAL_WINDOW_START`).
        let after = match parse_time(repo::sync_state::get(&store, INITIAL_WINDOW_START)?) {
            Some(t) => Some(t),
            None => {
                let start = self.window_start();
                if let Some(t) = start {
                    repo::sync_state::set(&store, INITIAL_WINDOW_START, &t.to_rfc3339())?;
                }
                start
            }
        };
        let budget = self.options.pages_per_run;
        let mut pages_this_run = 0u32;
        let mut run_threads: HashSet<ThreadId> = HashSet::new();
        let mut finished = false;
        loop {
            if cancel.is_cancelled() {
                rinfo!(
                    Category::Sync,
                    "initial sync {} cancelled after {} messages",
                    self.dig.id,
                    listed
                );
                break;
            }
            if budget > 0 && pages_this_run >= budget {
                rdebug!(
                    Category::Sync,
                    "initial sync {} pauses after {} pages, {} listed",
                    self.dig.id,
                    pages_this_run,
                    listed
                );
                break;
            }
            progress(SyncProgress {
                dig_id: self.dig.id.clone(),
                phase: SyncPhase::Listing,
                done: listed,
                total,
                step: "Listing messages".into(),
            });
            let page = self
                .provider
                .list_message_ids(&ListPage {
                    after,
                    page_token: page_token.clone(),
                    max: self.options.page_size,
                    ..Default::default()
                })
                .await?;
            if let Some(estimate) = page.result_size_estimate {
                if total != Some(estimate) {
                    repo::sync_state::set(&store, INITIAL_TOTAL, &estimate.to_string())?;
                }
                total = Some(estimate);
            }
            let ids: Vec<String> = page.refs.iter().map(|r| r.provider_id.clone()).collect();
            let messages = self
                .provider
                .get_messages_batch(&ids, Detail::Metadata)
                .await?;
            let (n, threads) = self.write_page(&store, &messages)?;
            report.added += n;
            listed += ids.len() as u64;
            page_token = page.next_page_token;
            // One checkpoint per page: the page's messages and the token land together.
            repo::sync_state::set(&store, INITIAL_LISTED, &listed.to_string())?;
            if n > 0 {
                repo::sync_state::set(&store, BODIES_PENDING, "1")?;
            }
            match &page_token {
                Some(t) => repo::sync_state::set(&store, INITIAL_PAGE_TOKEN, t)?,
                None => repo::sync_state::clear(&store, INITIAL_PAGE_TOKEN)?,
            }
            if let Some(oldest) = messages.iter().map(|m| m.date).min() {
                repo::sync_state::set(
                    &store,
                    repo::sync_state::OLDEST_SYNCED,
                    &oldest.to_rfc3339(),
                )?;
            }
            self.after_page(&store, &threads, n);
            run_threads.extend(threads);
            pages_this_run += 1;
            progress(SyncProgress {
                dig_id: self.dig.id.clone(),
                phase: SyncPhase::Metadata,
                done: listed,
                total,
                step: "Fetching headers".into(),
            });
            if page_token.is_none() {
                finished = true;
                break;
            }
        }
        report.listed = listed;
        report.total = total;
        if !finished {
            // The run's addresses now; the whole Dig when the listing ends.
            self.refresh_people(&store, &run_threads);
            report.complete = false;
            return Ok(report);
        }
        repo::sync_state::set(&store, repo::sync_state::INITIAL_SYNC_DONE, "1")?;
        repo::sync_state::set(
            &store,
            repo::sync_state::LAST_SYNC_AT,
            &Utc::now().to_rfc3339(),
        )?;
        report.cursor = repo::sync_state::get(&store, repo::sync_state::HISTORY_CURSOR)?;
        self.rebuild_people();
        progress(SyncProgress {
            dig_id: self.dig.id.clone(),
            phase: SyncPhase::Done,
            done: listed,
            total: Some(listed),
            step: "Headers complete".into(),
        });
        rinfo!(
            Category::Sync,
            "initial sync {} complete: {} messages",
            self.dig.id,
            listed
        );
        self.events.emit(RiddleEvent::SummaryChanged);
        Ok(report)
    }

    /// Older mail than the first sync's window, newest first, in the same
    /// bounded runs as the initial sync, resumable from its token. Each
    /// page comes with its bodies: the body fetch works newest first and
    /// would reach history last, and older mail is sifted once, with what
    /// it has. Done when the listing ends, or at once when nothing lies
    /// between the Dig's `history_days` floor and the window.
    pub async fn backfill_history(
        &self,
        progress: ProgressFn<'_>,
        cancel: &CancelFlag,
    ) -> Result<SyncReport> {
        let _t = Timed::start(
            Category::Sync,
            format!("history backfill {}", self.dig.id),
            std::time::Duration::from_secs(600),
        );
        let store = self.store()?;
        let mut report = SyncReport::new(SyncKind::Backfill);
        if repo::sync_state::get(&store, BACKFILL_DONE)?.as_deref() == Some("1") {
            return Ok(report);
        }
        let Some(before) = window_start_or_derived(&store)? else {
            // The first sync listed everything, or the Dig holds nothing:
            // there is no "before".
            repo::sync_state::set(&store, BACKFILL_DONE, "1")?;
            return Ok(report);
        };
        let floor = match repo::sync_state::get(&store, BACKFILL_FLOOR)? {
            Some(s) if s == "none" => None,
            Some(s) => parse_time(Some(s)),
            None => {
                let floor = self
                    .dig
                    .history_days
                    .map(|d| Utc::now() - Duration::days(i64::from(d)));
                let value = floor
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "none".into());
                repo::sync_state::set(&store, BACKFILL_FLOOR, &value)?;
                floor
            }
        };
        let mut listed: u64 =
            parse_count(repo::sync_state::get(&store, BACKFILL_LISTED)?).unwrap_or(0);
        let mut total: Option<u64> = parse_count(repo::sync_state::get(&store, BACKFILL_TOTAL)?);
        let mut oldest = parse_time(repo::sync_state::get(&store, BACKFILL_OLDEST)?);
        let step = |oldest: Option<DateTime<Utc>>| match oldest {
            Some(t) => format!("through {}", t.format("%B %Y")),
            None => "Listing older mail".to_string(),
        };
        let mut finished = floor.is_some_and(|f| f >= before);
        let mut page_token = repo::sync_state::get(&store, BACKFILL_PAGE_TOKEN)?;
        let budget = self.options.pages_per_run;
        let mut pages_this_run = 0u32;
        let mut run_threads: HashSet<ThreadId> = HashSet::new();
        while !finished {
            if cancel.is_cancelled() {
                rinfo!(
                    Category::Sync,
                    "history backfill {} cancelled after {} messages",
                    self.dig.id,
                    listed
                );
                break;
            }
            if budget > 0 && pages_this_run >= budget {
                rdebug!(
                    Category::Sync,
                    "history backfill {} pauses after {} pages, {} listed",
                    self.dig.id,
                    pages_this_run,
                    listed
                );
                break;
            }
            progress(SyncProgress {
                dig_id: self.dig.id.clone(),
                phase: SyncPhase::Backfill,
                done: listed,
                total,
                step: step(oldest),
            });
            let page = self
                .provider
                .list_message_ids(&ListPage {
                    after: floor,
                    before: Some(before),
                    page_token: page_token.clone(),
                    max: self.options.page_size,
                    ..Default::default()
                })
                .await?;
            if let Some(estimate) = page.result_size_estimate {
                if total != Some(estimate) {
                    repo::sync_state::set(&store, BACKFILL_TOTAL, &estimate.to_string())?;
                }
                total = Some(estimate);
            }
            let ids: Vec<String> = page.refs.iter().map(|r| r.provider_id.clone()).collect();
            let mut messages = Vec::with_capacity(ids.len());
            for chunk in ids.chunks(self.options.body_batch.max(1) as usize) {
                let full = self
                    .provider
                    .get_messages_batch(chunk, Detail::Full)
                    .await?;
                messages.extend(full.into_iter().map(with_body_text));
            }
            let (n, threads) = self.write_page(&store, &messages)?;
            report.added += n;
            listed += ids.len() as u64;
            page_token = page.next_page_token;
            repo::sync_state::set(&store, BACKFILL_LISTED, &listed.to_string())?;
            match &page_token {
                Some(t) => repo::sync_state::set(&store, BACKFILL_PAGE_TOKEN, t)?,
                None => repo::sync_state::clear(&store, BACKFILL_PAGE_TOKEN)?,
            }
            if let Some(min) = messages.iter().map(|m| m.date).min() {
                if oldest.is_none_or(|o| min < o) {
                    oldest = Some(min);
                    repo::sync_state::set(&store, BACKFILL_OLDEST, &min.to_rfc3339())?;
                }
            }
            self.after_page(&store, &threads, n);
            run_threads.extend(threads);
            pages_this_run += 1;
            if page_token.is_none() {
                finished = true;
            }
        }
        report.listed = listed;
        report.total = total;
        if !finished {
            self.refresh_people(&store, &run_threads);
            report.complete = false;
            progress(SyncProgress {
                dig_id: self.dig.id.clone(),
                phase: SyncPhase::Backfill,
                done: listed,
                total,
                step: step(oldest),
            });
            return Ok(report);
        }
        repo::sync_state::set(&store, BACKFILL_DONE, "1")?;
        if listed > 0 {
            self.rebuild_people();
        }
        progress(SyncProgress {
            dig_id: self.dig.id.clone(),
            phase: SyncPhase::Done,
            done: listed,
            total: Some(listed),
            step: "Older mail complete".into(),
        });
        rinfo!(
            Category::Sync,
            "history backfill {} complete: {} messages",
            self.dig.id,
            listed
        );
        if listed > 0 {
            self.events.emit(RiddleEvent::SummaryChanged);
        }
        Ok(report)
    }

    /// Write one page in a single transaction; returns the count and the threads touched.
    fn write_page(&self, store: &Store, messages: &[Message]) -> Result<(u64, HashSet<ThreadId>)> {
        let mut threads = HashSet::new();
        store.write(|c| {
            for m in messages {
                repo::messages::upsert_in(c, m)?;
                threads.insert(m.thread_id.clone());
            }
            Ok(())
        })?;
        Ok((messages.len() as u64, threads))
    }

    /// Bookkeeping after a page: Thread State for the threads it touched
    /// (Sift reads it as pages land) and an event. Never fails the sync.
    /// People wait for `refresh_people` at the end of the run.
    fn after_page(&self, store: &Store, threads: &HashSet<ThreadId>, count: u64) {
        if threads.is_empty() {
            return;
        }
        let now = Utc::now();
        match self.profile() {
            Ok(profile) => {
                let list: Vec<ThreadId> = threads.iter().cloned().collect();
                if let Err(e) =
                    riddle_context::update_thread_states(store, &self.dig.id, &list, &profile, now)
                {
                    swallowed!(
                        Category::Sync,
                        "thread_states_after_page",
                        e,
                        "thread state update failed for dig {}",
                        self.dig.id
                    );
                }
            }
            Err(e) => swallowed!(
                Category::Sync,
                "profile_for_page",
                e,
                "profile unavailable for dig {}",
                self.dig.id
            ),
        }
        if count > 0 {
            self.events.emit(RiddleEvent::MessagesChanged {
                dig_id: self.dig.id.clone(),
                count,
            });
        }
    }

    /// The People of a run's threads, in one batch. Bookkeeping.
    fn refresh_people(&self, store: &Store, threads: &HashSet<ThreadId>) {
        if threads.is_empty() {
            return;
        }
        let _ = store;
        let result = self.profile().and_then(|profile| {
            let list: Vec<ThreadId> = threads.iter().cloned().collect();
            riddle_people::update_for_threads(
                &self.stores.index(),
                &self.stores.open_digs(),
                &self.dig.id,
                &list,
                &profile,
                Utc::now(),
            )
        });
        if let Err(e) = result {
            swallowed!(
                Category::Sync,
                "people_for_run",
                e,
                "people update failed for dig {}",
                self.dig.id
            );
        }
    }

    /// Every Person with mail in this Dig, once, when a listing finishes:
    /// a restart mid-listing loses at most one run's addresses, and this
    /// covers them. Bookkeeping.
    fn rebuild_people(&self) {
        let result = self.profile().and_then(|profile| {
            riddle_people::rebuild_for_dig(
                &self.stores.index(),
                &self.stores.open_digs(),
                &self.dig.id,
                &profile,
                Utc::now(),
            )
        });
        if let Err(e) = result {
            swallowed!(
                Category::Sync,
                "people_rebuild",
                e,
                "people rebuild failed for dig {}",
                self.dig.id
            );
        }
    }

    /// Fetch up to `limit` bodies, newest first. Returns how many were stored.
    pub async fn fetch_bodies(
        &self,
        limit: u32,
        progress: ProgressFn<'_>,
        cancel: &CancelFlag,
    ) -> Result<u64> {
        let store = self.store()?;
        let ids = repo::messages::ids_without_body(&store, limit)?;
        if ids.is_empty() {
            repo::sync_state::clear(&store, BODIES_PENDING)?;
            return Ok(0);
        }
        let remaining = repo::messages::count_without_body(&store)?;
        let mut done = 0u64;
        for chunk in ids.chunks(self.options.body_batch.max(1) as usize) {
            if cancel.is_cancelled() {
                break;
            }
            let provider_ids: Vec<String> = chunk
                .iter()
                .map(|id| id.provider_id().to_string())
                .collect();
            let full = self
                .provider
                .get_messages_batch(&provider_ids, Detail::Full)
                .await?;
            let mut fetched: HashSet<MessageId> = HashSet::new();
            store.write(|c| {
                for m in full.iter().cloned().map(with_body_text) {
                    repo::messages::upsert_in(c, &m)?;
                    fetched.insert(m.id.clone());
                }
                Ok(())
            })?;
            // A message the provider no longer has must not be refetched forever.
            for id in chunk.iter().filter(|id| !fetched.contains(id)) {
                repo::messages::set_body(&store, id, Some(""), None)?;
            }
            done += full.len() as u64;
            progress(SyncProgress {
                dig_id: self.dig.id.clone(),
                phase: SyncPhase::Bodies,
                done,
                total: Some(remaining),
                step: "Fetching bodies".into(),
            });
        }
        if repo::messages::count_without_body(&store)? == 0 {
            repo::sync_state::clear(&store, BODIES_PENDING)?;
        }
        rdebug!(
            Category::Sync,
            "dig {} fetched {} bodies",
            self.dig.id,
            done
        );
        if done > 0 {
            self.events.emit(RiddleEvent::MessagesChanged {
                dig_id: self.dig.id.clone(),
                count: done,
            });
        }
        Ok(done)
    }

    /// Follow the history cursor. Expiry falls through to a full resync.
    pub async fn incremental(
        &self,
        progress: ProgressFn<'_>,
        cancel: &CancelFlag,
        on_external: ExternalFn<'_>,
    ) -> Result<SyncReport> {
        let store = self.store()?;
        let Some(cursor) = repo::sync_state::get(&store, repo::sync_state::HISTORY_CURSOR)? else {
            rwarn!(
                Category::Sync,
                "dig {} has no history cursor; resyncing",
                self.dig.id
            );
            return self.full_resync(progress, cancel).await;
        };
        let page = match self.provider.history(&cursor).await {
            Ok(p) if p.expired => {
                rwarn!(
                    Category::Sync,
                    "dig {} history cursor expired; resyncing",
                    self.dig.id
                );
                return self.full_resync(progress, cancel).await;
            }
            Ok(p) => p,
            Err(e) if e.kind == ErrorKind::ProviderHistoryExpired => {
                rwarn!(
                    Category::Sync,
                    "dig {} history cursor expired ({}); resyncing",
                    self.dig.id,
                    e.kind.as_str()
                );
                return self.full_resync(progress, cancel).await;
            }
            Err(e) => return Err(e),
        };
        let mut report = SyncReport::new(SyncKind::Incremental);
        let total = page.changes.len() as u64;
        progress(SyncProgress {
            dig_id: self.dig.id.clone(),
            phase: SyncPhase::Incremental,
            done: 0,
            total: Some(total),
            step: "Applying changes".into(),
        });

        // Coalesce: an id added then deleted in the same page is nothing.
        let mut added: Vec<String> = Vec::new();
        let mut deleted: HashSet<String> = HashSet::new();
        let mut label_changes: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
        for ch in &page.changes {
            match ch {
                Change::Added { id } => {
                    if !added.contains(id) {
                        added.push(id.clone());
                    }
                }
                Change::Deleted { id } => {
                    deleted.insert(id.clone());
                }
                Change::LabelsChanged {
                    id,
                    added: a,
                    removed: r,
                } => label_changes.push((id.clone(), a.clone(), r.clone())),
            }
        }
        added.retain(|id| !deleted.contains(id));
        label_changes.retain(|(id, _, _)| !deleted.contains(id));

        let mut threads: HashSet<ThreadId> = HashSet::new();
        if !added.is_empty() {
            let messages = self
                .provider
                .get_messages_batch(&added, Detail::Metadata)
                .await?;
            let (n, t) = self.write_page(&store, &messages)?;
            report.added += n;
            threads.extend(t);
            if n > 0 {
                repo::sync_state::set(&store, BODIES_PENDING, "1")?;
            }
        }
        for id in &deleted {
            let mid = MessageId::for_provider(&self.dig.id, id);
            if let Some(m) = repo::messages::get(&store, &self.dig.id, &mid)? {
                threads.insert(m.thread_id.clone());
                repo::messages::mark_deleted(&store, &mid)?;
                report.deleted += 1;
            }
        }
        if !label_changes.is_empty() {
            let mut resolver = self.resolver.lock().await;
            if resolver.id_of("INBOX").is_none() {
                resolver.seed(&self.provider.list_labels().await?);
            }
            // One read of the changed messages' labels, the changes applied
            // in order in memory, one write of the rows that differ.
            let ids: Vec<MessageId> = label_changes
                .iter()
                .map(|(id, _, _)| MessageId::for_provider(&self.dig.id, id))
                .collect();
            let mut known: HashMap<MessageId, (ThreadId, Vec<String>)> =
                repo::messages::labels_for_ids(&store, &ids)?
                    .into_iter()
                    .map(|(id, thread, labels)| (id, (thread, labels)))
                    .collect();
            let mut changed: Vec<MessageId> = Vec::new();
            let mut unseen: Vec<String> = Vec::new();
            let mut to_attribute: Vec<(MessageId, ThreadId, Vec<String>, Vec<String>)> = Vec::new();
            for (id, a, r) in label_changes {
                if cancel.is_cancelled() {
                    report.complete = false;
                    break;
                }
                let mid = MessageId::for_provider(&self.dig.id, &id);
                let Some((thread, labels)) = known.get_mut(&mid) else {
                    // Label change for a message we have not seen: fetch it below.
                    if !unseen.contains(&id) {
                        unseen.push(id);
                    }
                    continue;
                };
                // Apply to the store: the provider is authoritative for labels.
                let before = labels.clone();
                for x in &a {
                    if !labels.contains(x) {
                        labels.push(x.clone());
                    }
                }
                labels.retain(|l| !r.contains(l));
                if *labels != before && !changed.contains(&mid) {
                    changed.push(mid.clone());
                }
                report.label_changes += 1;
                threads.insert(thread.clone());
                // Attribution.
                let a_nv: Vec<String> = a
                    .iter()
                    .filter(|l| !is_volatile_label(l))
                    .cloned()
                    .collect();
                let r_nv: Vec<String> = r
                    .iter()
                    .filter(|l| !is_volatile_label(l))
                    .cloned()
                    .collect();
                if a_nv.is_empty() && r_nv.is_empty() {
                    continue;
                }
                to_attribute.push((mid, thread.clone(), a_nv, r_nv));
            }
            let writes: Vec<(MessageId, Vec<String>)> = changed
                .iter()
                .filter_map(|id| known.get(id).map(|(_, l)| (id.clone(), l.clone())))
                .collect();
            report.updated += writes.len() as u64;
            repo::messages::set_labels_many(&store, &writes)?;
            if !unseen.is_empty() {
                let fetched = self
                    .provider
                    .get_messages_batch(&unseen, Detail::Metadata)
                    .await?;
                let (n, t) = self.write_page(&store, &fetched)?;
                report.added += n;
                threads.extend(t);
            }
            for (mid, thread, a_nv, r_nv) in to_attribute {
                if self
                    .is_echo(&store, &mid, &a_nv, &r_nv, &mut resolver)
                    .await?
                {
                    report.echoes += 1;
                    rdebug!(
                        Category::Sync,
                        "label change on {} attributed to projection echo",
                        mid
                    );
                } else {
                    rinfo!(
                        Category::Sync,
                        "external label change on {} (+{} -{})",
                        mid,
                        a_nv.len(),
                        r_nv.len()
                    );
                    let mv = ExternalMove {
                        dig_id: self.dig.id.clone(),
                        message_id: mid,
                        thread_id: thread,
                        added: a_nv,
                        removed: r_nv,
                        at: Utc::now(),
                    };
                    on_external(mv.clone());
                    report.external_moves.push(mv);
                }
            }
        }
        if let Some(next) = &page.next_cursor {
            repo::sync_state::set(&store, repo::sync_state::HISTORY_CURSOR, next)?;
            report.cursor = Some(next.clone());
        }
        repo::sync_state::set(
            &store,
            repo::sync_state::LAST_SYNC_AT,
            &Utc::now().to_rfc3339(),
        )?;
        self.after_page(
            &store,
            &threads,
            report.added + report.updated + report.deleted,
        );
        self.refresh_people(&store, &threads);
        if !threads.is_empty() {
            self.events.emit(RiddleEvent::SummaryChanged);
        }
        progress(SyncProgress {
            dig_id: self.dig.id.clone(),
            phase: SyncPhase::Done,
            done: total,
            total: Some(total),
            step: "Up to date".into(),
        });
        Ok(report)
    }

    /// Does the change match a projection row applied recently to this message?
    async fn is_echo(
        &self,
        store: &Store,
        mid: &MessageId,
        added: &[String],
        removed: &[String],
        resolver: &mut LabelResolver,
    ) -> Result<bool> {
        let rows = repo::outbox::recently_applied_for_message::<ProjectionOp>(
            store,
            mid.as_str(),
            self.options.echo_window.num_milliseconds(),
        )?;
        for row in rows {
            let op = &row.payload;
            let mut add_ids: Vec<String> = Vec::new();
            for name in &op.add {
                add_ids.push(resolver.resolve(self.provider.as_ref(), name).await?);
            }
            let mut remove_ids: Vec<String> = Vec::new();
            for name in &op.remove {
                remove_ids.push(resolver.resolve(self.provider.as_ref(), name).await?);
            }
            let adds_ok = added.iter().all(|l| add_ids.contains(l));
            let removes_ok = removed.iter().all(|l| {
                remove_ids.contains(l) || (op.remove_managed && resolver.is_managed_id(l))
            });
            if adds_ok && removes_ok {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// List everything again, reconcile labels against the expected
    /// projection, report differences, never overwrite (§15).
    pub async fn full_resync(
        &self,
        progress: ProgressFn<'_>,
        cancel: &CancelFlag,
    ) -> Result<SyncReport> {
        let _t = Timed::start(
            Category::Sync,
            format!("resync {}", self.dig.id),
            std::time::Duration::from_secs(600),
        );
        let store = self.store()?;
        repo::sync_state::set(&store, RESYNC_REQUIRED, "1")?;
        let mut report = SyncReport::new(SyncKind::Resync);
        let profile = self.provider.profile().await?;
        let after = self.window_start();
        let known: Vec<(String, MessageId, Vec<String>)> = match after {
            Some(a) => repo::messages::provider_labels_since(&store, &a)?,
            None => repo::messages::provider_labels_since(&store, &DateTime::<Utc>::UNIX_EPOCH)?,
        };
        let mut seen: HashSet<String> = HashSet::new();
        let mut page_token: Option<String> = None;
        let mut listed = 0u64;
        let mut total: Option<u64> = None;
        let mut threads: HashSet<ThreadId> = HashSet::new();
        let mut resolver = self.resolver.lock().await;
        if resolver.id_of("INBOX").is_none() {
            resolver.seed(&self.provider.list_labels().await?);
        }
        loop {
            if cancel.is_cancelled() {
                report.complete = false;
                return Ok(report);
            }
            progress(SyncProgress {
                dig_id: self.dig.id.clone(),
                phase: SyncPhase::Resync,
                done: listed,
                total,
                step: "Resyncing".into(),
            });
            let page = self
                .provider
                .list_message_ids(&ListPage {
                    after,
                    page_token: page_token.clone(),
                    max: self.options.page_size,
                    ..Default::default()
                })
                .await?;
            total = page.result_size_estimate.or(total);
            let ids: Vec<String> = page.refs.iter().map(|r| r.provider_id.clone()).collect();
            let messages = self
                .provider
                .get_messages_batch(&ids, Detail::Metadata)
                .await?;
            for m in &messages {
                seen.insert(m.provider_id.clone());
                if let Some(k) = repo::classifications::current(&store, &m.id)? {
                    let plan = riddle_projection::expected_managed_labels(&self.dig, Some(&k));
                    let mut missing = Vec::new();
                    for name in &plan.add {
                        let id = resolver.resolve(self.provider.as_ref(), name).await?;
                        if !m.labels.contains(&id) {
                            missing.push(name.clone());
                        }
                    }
                    let mut present = Vec::new();
                    for name in &plan.remove {
                        let id = resolver.resolve(self.provider.as_ref(), name).await?;
                        if m.labels.contains(&id) {
                            present.push(name.clone());
                        }
                    }
                    if !missing.is_empty() || !present.is_empty() {
                        report.differences.push(Difference {
                            message_id: m.id.clone(),
                            expected_add_missing: missing,
                            expected_remove_present: present,
                        });
                    }
                }
                let existed = repo::messages::labels_of(&store, &m.id)?;
                if existed.is_empty() {
                    report.added += 1;
                } else if existed != m.labels {
                    report.updated += 1;
                }
            }
            let (_, t) = self.write_page(&store, &messages)?;
            threads.extend(t);
            listed += ids.len() as u64;
            if !messages.is_empty() {
                repo::sync_state::set(&store, BODIES_PENDING, "1")?;
            }
            self.after_page(&store, &threads, messages.len() as u64);
            self.refresh_people(&store, &threads);
            threads.clear();
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        // Anything we knew in the window that the provider no longer lists is gone.
        for (pid, mid, _) in known {
            if !seen.contains(&pid) {
                repo::messages::mark_deleted(&store, &mid)?;
                report.deleted += 1;
            }
        }
        if let Some(c) = profile.history_cursor {
            repo::sync_state::set(&store, repo::sync_state::HISTORY_CURSOR, &c)?;
            report.cursor = Some(c);
        }
        repo::sync_state::set(&store, repo::sync_state::INITIAL_SYNC_DONE, "1")?;
        repo::sync_state::clear(&store, INITIAL_PAGE_TOKEN)?;
        repo::sync_state::clear(&store, RESYNC_REQUIRED)?;
        repo::sync_state::set(
            &store,
            repo::sync_state::LAST_SYNC_AT,
            &Utc::now().to_rfc3339(),
        )?;
        rinfo!(
            Category::Sync,
            "resync {} complete: {} listed, {} differences, {} deleted",
            self.dig.id,
            listed,
            report.differences.len(),
            report.deleted
        );
        progress(SyncProgress {
            dig_id: self.dig.id.clone(),
            phase: SyncPhase::Done,
            done: listed,
            total: Some(listed),
            step: "Resync complete".into(),
        });
        self.events.emit(RiddleEvent::SummaryChanged);
        Ok(report)
    }
}

impl std::fmt::Debug for SyncEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncEngine")
            .field("dig", &self.dig.id)
            .finish()
    }
}
