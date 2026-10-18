use crate::engine::{
    window_start_or_derived, CancelFlag, SyncEngine, SyncKind, SyncOptions, SyncPhase,
    SyncProgress, BACKFILL_DONE, BACKFILL_OLDEST, BODIES_PENDING, INITIAL_LISTED, INITIAL_TOTAL,
    INITIAL_WINDOW_START,
};
use riddle_core::*;
use riddle_mail::{FixtureProvider, FixtureSpec, MailProvider, ProviderRegistry};
use riddle_projection::{enqueue_classification, OutboxProcessor, SPOIL_LABEL};
use riddle_store::{repo, Stores};
use std::sync::{Arc, Mutex};

struct H {
    stores: Arc<Stores>,
    dig: Dig,
    fixture: Arc<FixtureProvider>,
    engine: Arc<SyncEngine>,
    events: Arc<RecordingSink>,
    registry: Arc<ProviderRegistry>,
}

fn options() -> SyncOptions {
    SyncOptions {
        page_size: 25,
        window_days: Some(90),
        body_batch: 20,
        ..Default::default()
    }
}

fn harness(count: u32) -> H {
    harness_with(count, 60, None, options())
}

/// A fixture mailbox of `count` messages over `days` days, a Dig with the
/// given `history_days`, and an engine with `options`.
fn harness_with(count: u32, days: u32, history_days: Option<u32>, options: SyncOptions) -> H {
    let stores = Arc::new(Stores::in_memory().unwrap());
    let dig = Dig {
        id: DigId::from("dig-s"),
        name: "Work".into(),
        email: "matt@example.com".into(),
        provider: ProviderKind::Fixture,
        group: None,
        privacy: PrivacyMode::CloudJudgment,
        projection: ProjectionMode::Full,
        status: DigStatus::NotConnected,
        shadow_started_at: None,
        created_at: chrono::Utc::now(),
        disabled: false,
        history_days,
        mailbox: None,
    };
    repo::digs::upsert(&stores.index(), &dig).unwrap();
    repo::user_profile::put(
        &stores.index(),
        &UserProfile {
            addresses: vec!["matt@example.com".into()],
            organisation_domains: vec!["example.com".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let fixture = Arc::new(FixtureProvider::new(
        dig.id.clone(),
        FixtureSpec {
            count,
            seed: 21,
            days,
            ..Default::default()
        },
    ));
    let registry = Arc::new(ProviderRegistry::new(stores.clone()));
    registry.insert_fixture(dig.id.clone(), fixture.clone());
    let events = Arc::new(RecordingSink::default());
    let engine = Arc::new(
        SyncEngine::new(dig.clone(), fixture.clone(), stores.clone(), events.clone())
            .with_options(options),
    );
    H {
        stores,
        dig,
        fixture,
        engine,
        events,
        registry,
    }
}

impl H {
    /// A new engine over the same stores and provider: what a restart makes.
    fn restarted_engine(&self) -> Arc<SyncEngine> {
        Arc::new(
            SyncEngine::new(
                self.dig.clone(),
                self.fixture.clone(),
                self.stores.clone(),
                self.events.clone(),
            )
            .with_options(self.engine.options().clone()),
        )
    }

    /// The fixture's listed messages (not spam) dated on or after `since`.
    fn listed_since(&self, since: Option<chrono::DateTime<chrono::Utc>>) -> u64 {
        self.fixture
            .snapshot()
            .iter()
            .filter(|m| !m.labels.iter().any(|l| l == "SPAM"))
            .filter(|m| since.is_none_or(|s| m.date >= s))
            .count() as u64
    }
}

fn no_progress() -> impl FnMut(SyncProgress) + Send {
    |_| {}
}

#[tokio::test]
async fn a_full_sync_of_the_fixture_lands_every_listed_message_with_thread_state_and_people() {
    let h = harness(150);
    let mut phases = Vec::new();
    let mut progress = |p: SyncProgress| phases.push(p.phase);
    let cancel = CancelFlag::new();
    let report = h.engine.initial_sync(&mut progress, &cancel).await.unwrap();
    assert_eq!(report.kind, SyncKind::Initial);
    assert!(report.complete);
    let store = h.stores.dig(&h.dig.id).unwrap();
    let expected = h
        .fixture
        .snapshot()
        .iter()
        .filter(|m| !m.labels.iter().any(|l| l == "SPAM"))
        .count() as u64;
    assert_eq!(report.added, expected);
    assert_eq!(repo::messages::count(&store).unwrap(), expected);
    assert_eq!(
        repo::messages::count_without_body(&store).unwrap(),
        expected,
        "metadata first, bodies later"
    );
    assert!(
        phases.contains(&SyncPhase::Listing)
            && phases.contains(&SyncPhase::Metadata)
            && phases.last() == Some(&SyncPhase::Done)
    );
    assert_eq!(
        repo::sync_state::get(&store, repo::sync_state::INITIAL_SYNC_DONE)
            .unwrap()
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        repo::sync_state::get(&store, repo::sync_state::HISTORY_CURSOR).unwrap(),
        Some(h.fixture.current_cursor())
    );
    // Thread State and People followed the pages.
    let sent = h
        .fixture
        .snapshot()
        .into_iter()
        .find(|m| m.is_sent && m.headers.in_reply_to.is_none())
        .unwrap();
    let t = repo::thread_state::get(&store, &sent.thread_id)
        .unwrap()
        .expect("thread state for a sent thread");
    assert_eq!(t.last_speaker, Speaker::Me);
    assert!(
        !repo::thread_state::with_open_asks_from_me(&store)
            .unwrap()
            .is_empty(),
        "Follow Up material exists"
    );
    assert!(
        riddle_people::lookup(&h.stores.index(), "priya@example.com")
            .unwrap()
            .is_some()
    );
    assert!(h
        .events
        .take()
        .iter()
        .any(|e| matches!(e, RiddleEvent::MessagesChanged { .. })));
    // Bodies arrive lazily, newest first.
    let n = h
        .engine
        .fetch_bodies(30, &mut no_progress(), &cancel)
        .await
        .unwrap();
    assert_eq!(n, 30);
    assert_eq!(
        repo::messages::count_without_body(&store).unwrap(),
        expected - 30
    );
    let newest = repo::messages::list(
        &store,
        &h.dig.id,
        &Default::default(),
        &PageRequest {
            cursor: None,
            limit: Some(1),
        },
    )
    .unwrap()
    .items
    .remove(0);
    assert!(
        repo::messages::get(&store, &h.dig.id, &newest.id)
            .unwrap()
            .unwrap()
            .has_body
    );
}

#[tokio::test]
async fn a_cancelled_initial_sync_resumes_from_its_checkpoint() {
    let h = harness(120);
    let cancel = CancelFlag::new();
    let c2 = cancel.clone();
    let mut pages = 0;
    let mut progress = move |p: SyncProgress| {
        if p.phase == SyncPhase::Metadata {
            pages += 1;
            if pages == 2 {
                c2.cancel();
            }
        }
    };
    let first = h.engine.initial_sync(&mut progress, &cancel).await.unwrap();
    assert!(!first.complete);
    assert_eq!(first.added, 50);
    let store = h.stores.dig(&h.dig.id).unwrap();
    assert!(
        repo::sync_state::get(&store, crate::engine::INITIAL_PAGE_TOKEN)
            .unwrap()
            .is_some()
    );
    assert!(!h.engine.initial_sync_done().unwrap());
    let calls_before = h.fixture.call_count("get_batch");
    let second = h
        .engine
        .sync_once(&mut no_progress(), &CancelFlag::new(), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(second.kind, SyncKind::Initial);
    assert!(second.complete);
    let expected = h
        .fixture
        .snapshot()
        .iter()
        .filter(|m| !m.labels.iter().any(|l| l == "SPAM"))
        .count() as u64;
    assert_eq!(
        first.added + second.added,
        expected,
        "no page was fetched twice"
    );
    assert!(h.fixture.call_count("get_batch") - calls_before < 5);
    assert_eq!(repo::messages::count(&store).unwrap(), expected);
}

#[tokio::test]
async fn an_injected_message_arrives_through_the_next_incremental_pass() {
    let h = harness(60);
    let cancel = CancelFlag::new();
    h.engine
        .initial_sync(&mut no_progress(), &cancel)
        .await
        .unwrap();
    let store = h.stores.dig(&h.dig.id).unwrap();
    let before = repo::messages::count(&store).unwrap();
    let quiet = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(quiet.kind, SyncKind::Incremental);
    assert_eq!(quiet.added, 0);
    let injected = h
        .fixture
        .inject(
            "personal_question",
            None,
            Some("Dinner Sunday?".into()),
            Some("Are you coming for dinner on Sunday?".into()),
        )
        .unwrap();
    let report = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(report.added, 1);
    assert_eq!(repo::messages::count(&store).unwrap(), before + 1);
    let stored = repo::messages::get(&store, &h.dig.id, &injected.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.subject, "Dinner Sunday?");
    let t = repo::thread_state::get(&store, &stored.thread_id)
        .unwrap()
        .unwrap();
    assert_eq!(t.last_speaker, Speaker::Them);
    assert_eq!(
        t.open_asks_from_them,
        vec!["Are you coming for dinner on Sunday?".to_string()]
    );
    assert_eq!(
        repo::sync_state::get(&store, repo::sync_state::HISTORY_CURSOR).unwrap(),
        Some(h.fixture.current_cursor())
    );
    // A deletion elsewhere is honoured.
    h.fixture.delete_message(&injected.provider_id);
    let report = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(report.deleted, 1);
    assert!(repo::messages::get(&store, &h.dig.id, &injected.id)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_projection_echo_is_not_an_external_move_but_a_move_from_another_client_is() {
    let h = harness(60);
    let cancel = CancelFlag::new();
    h.engine
        .initial_sync(&mut no_progress(), &cancel)
        .await
        .unwrap();
    let store = h.stores.dig(&h.dig.id).unwrap();
    let inbox: Vec<Message> = h
        .fixture
        .snapshot()
        .into_iter()
        .filter(|m| m.labels.iter().any(|l| l == "INBOX") && !m.is_sent)
        .collect();
    let (ours, theirs) = (inbox[0].clone(), inbox[1].clone());

    // Riddle projects Spoil on `ours`.
    let k = Classification {
        id: ClassificationId::new(),
        message_id: ours.id.clone(),
        dig_id: h.dig.id.clone(),
        importance: Importance::default_for(&Disposition::Spoil),
        disposition: Disposition::Spoil,
        strata: vec![],
        notify: false,
        mechanism: DecisionMechanism::Mesh,
        mesh_id: None,
        mesh_version: None,
        matched_rule: None,
        confidence: None,
        reason: "t".into(),
        trace_id: TraceId::new(),
        shadow: false,
        created_at: chrono::Utc::now(),
        superseded_at: None,
        series: None,
    };
    repo::classifications::record(&store, &k).unwrap();
    enqueue_classification(&store, &h.dig, &ours, &k.disposition, &k.strata).unwrap();
    let processor = OutboxProcessor::new(h.stores.clone(), h.registry.clone());
    let applied = processor.drain(&h.dig, 3).await.unwrap();
    assert_eq!(applied.applied, 1);
    // A human archives `theirs` from the phone.
    h.fixture
        .modify_labels(
            std::slice::from_ref(&theirs.provider_id),
            &[],
            &["INBOX".into()],
        )
        .await
        .unwrap();
    // And marks something read, which is noise.
    h.fixture
        .modify_labels(
            std::slice::from_ref(&inbox[2].provider_id),
            &[],
            &["UNREAD".into()],
        )
        .await
        .unwrap();

    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    let report = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut move |m| {
            s2.lock().unwrap().push(m)
        })
        .await
        .unwrap();
    assert_eq!(report.label_changes, 3);
    assert_eq!(report.echoes, 1, "Riddle's own write echoed back");
    assert_eq!(report.external_moves.len(), 1);
    let external = &report.external_moves[0];
    assert_eq!(external.message_id, theirs.id);
    assert_eq!(external.removed, vec!["INBOX".to_string()]);
    assert_eq!(seen.lock().unwrap().len(), 1);
    // The store follows the provider for both.
    assert!(repo::messages::labels_of(&store, &ours.id)
        .unwrap()
        .contains(&SPOIL_LABEL.to_string()));
    assert!(!repo::messages::labels_of(&store, &theirs.id)
        .unwrap()
        .contains(&"INBOX".to_string()));
    assert!(!repo::messages::labels_of(&store, &inbox[2].id)
        .unwrap()
        .contains(&"UNREAD".to_string()));
}

#[tokio::test]
async fn an_expired_cursor_triggers_a_full_resync_that_reports_differences_without_overwriting() {
    let h = harness(60);
    let cancel = CancelFlag::new();
    h.engine
        .initial_sync(&mut no_progress(), &cancel)
        .await
        .unwrap();
    let store = h.stores.dig(&h.dig.id).unwrap();
    let m = h
        .fixture
        .snapshot()
        .into_iter()
        .find(|m| m.labels.iter().any(|l| l == "INBOX") && !m.is_sent)
        .unwrap();
    // Riddle believes this is Spoil, but the provider never got the projection.
    let k = Classification {
        id: ClassificationId::new(),
        message_id: m.id.clone(),
        dig_id: h.dig.id.clone(),
        importance: Importance::default_for(&Disposition::Spoil),
        disposition: Disposition::Spoil,
        strata: vec![],
        notify: false,
        mechanism: DecisionMechanism::Mesh,
        mesh_id: None,
        mesh_version: None,
        matched_rule: None,
        confidence: None,
        reason: "t".into(),
        trace_id: TraceId::new(),
        shadow: false,
        created_at: chrono::Utc::now(),
        superseded_at: None,
        series: None,
    };
    repo::classifications::record(&store, &k).unwrap();
    let gone = h
        .fixture
        .snapshot()
        .into_iter()
        .find(|x| x.id != m.id && x.labels.iter().any(|l| l == "INBOX"))
        .unwrap();
    h.fixture.delete_message(&gone.provider_id);
    h.fixture.expire_history();
    let report = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(report.kind, SyncKind::Resync);
    assert!(report.complete);
    let diff = report
        .differences
        .iter()
        .find(|d| d.message_id == m.id)
        .expect("the unprojected Spoil is reported");
    assert_eq!(diff.expected_add_missing, vec![SPOIL_LABEL.to_string()]);
    assert_eq!(diff.expected_remove_present, vec!["INBOX".to_string()]);
    assert!(
        h.fixture
            .labels_of(&m.provider_id)
            .unwrap()
            .contains(&"INBOX".to_string()),
        "resync never writes to the provider"
    );
    assert_eq!(report.deleted, 1);
    assert!(repo::messages::get(&store, &h.dig.id, &gone.id)
        .unwrap()
        .is_none());
    assert_eq!(
        repo::sync_state::get(&store, repo::sync_state::HISTORY_CURSOR).unwrap(),
        Some(h.fixture.current_cursor())
    );
    assert!(
        repo::sync_state::get(&store, crate::engine::RESYNC_REQUIRED)
            .unwrap()
            .is_none()
    );
    // Back to incremental afterwards.
    let next = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(next.kind, SyncKind::Incremental);
}

#[tokio::test]
async fn the_initial_sync_lists_pages_per_run_and_a_restarted_engine_resumes_from_its_token() {
    let h = harness_with(
        150,
        60,
        None,
        SyncOptions {
            pages_per_run: 2,
            ..options()
        },
    );
    let cancel = CancelFlag::new();
    let expected = h.listed_since(None);
    let store = h.stores.dig(&h.dig.id).unwrap();
    // Two pages of twenty-five, then the run ends with its token kept.
    let first = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(first.kind, SyncKind::Initial);
    assert!(!first.complete);
    assert_eq!((first.added, first.listed), (50, 50));
    assert_eq!(first.total, Some(expected), "the provider's estimate");
    assert!(!h.engine.initial_sync_done().unwrap());
    assert_eq!(
        repo::sync_state::get(&store, INITIAL_LISTED)
            .unwrap()
            .as_deref(),
        Some("50")
    );
    assert_eq!(
        repo::sync_state::get(&store, INITIAL_TOTAL)
            .unwrap()
            .as_deref(),
        Some(expected.to_string().as_str())
    );
    // A restart: a new engine over the same stores goes on from the token.
    let second = h
        .restarted_engine()
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(!second.complete);
    assert_eq!((second.added, second.listed), (50, 100));
    let third = h
        .restarted_engine()
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(third.complete);
    assert_eq!(third.listed, expected);
    assert_eq!(first.added + second.added + third.added, expected);
    assert_eq!(repo::messages::count(&store).unwrap(), expected);
    assert_eq!(h.fixture.call_count("list"), 6, "no page listed twice");
    assert!(h.engine.initial_sync_done().unwrap());
    // Steady state from here.
    let next = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!(next.kind, SyncKind::Incremental);
}

#[tokio::test]
async fn people_follow_each_run_of_the_first_sync_and_are_rebuilt_when_it_finishes() {
    let h = harness_with(
        150,
        60,
        None,
        SyncOptions {
            pages_per_run: 2,
            ..options()
        },
    );
    let cancel = CancelFlag::new();
    let index = h.stores.index();
    let listed: Vec<Message> = h
        .fixture
        .snapshot()
        .into_iter()
        .filter(|m| !m.labels.iter().any(|l| l == "SPAM"))
        .collect();
    let senders = |msgs: &[Message]| -> Vec<String> {
        let mut v: Vec<String> = msgs
            .iter()
            .filter(|m| !m.is_sent)
            .map(|m| m.from.email.to_lowercase())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let first_run = senders(&listed[..50]);
    // Anyone the first run's pages mention at all: a recipient of the
    // user's sent mail there is refreshed too.
    let mentioned: Vec<String> = listed[..50]
        .iter()
        .flat_map(|m| {
            std::iter::once(&m.from)
                .chain(m.to.iter())
                .chain(m.cc.iter())
                .map(|a| a.email.to_lowercase())
        })
        .collect();
    let later_only: Vec<String> = senders(&listed[50..])
        .into_iter()
        .filter(|s| !mentioned.contains(s))
        .collect();
    assert!(
        !later_only.is_empty(),
        "the fixture has senders beyond the first run"
    );
    let writes_before = index.stats().writes;
    let first = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(!first.complete);
    // The run's senders have People, from one batch at the end of the run,
    // not one refresh per page; a sender of later pages has none yet.
    for s in &first_run {
        assert!(riddle_people::lookup(&index, s).unwrap().is_some(), "{s}");
    }
    for s in &later_only {
        assert!(riddle_people::lookup(&index, s).unwrap().is_none(), "{s}");
    }
    let people_writes = index.stats().writes - writes_before;
    assert!(
        people_writes <= first_run.len() as u64 + 2,
        "one write per Person of the run, not per page: {people_writes}"
    );
    while !h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap()
        .complete
    {}
    for s in senders(&listed) {
        assert!(riddle_people::lookup(&index, &s).unwrap().is_some(), "{s}");
    }
}

#[tokio::test]
async fn bodies_are_pending_from_a_listing_until_the_fetch_has_nothing_left() {
    let h = harness_with(
        60,
        60,
        None,
        SyncOptions {
            pages_per_run: 1,
            ..options()
        },
    );
    let store = h.stores.dig(&h.dig.id).unwrap();
    let cancel = CancelFlag::new();
    let pending = || {
        repo::sync_state::get(&store, BODIES_PENDING)
            .unwrap()
            .is_some()
    };
    assert!(!pending(), "nothing listed yet");
    let first = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(!first.complete);
    assert!(
        pending(),
        "a page of headers landed; their bodies are to come"
    );
    // Twenty of the twenty-five: still behind.
    assert_eq!(
        h.engine
            .fetch_bodies(20, &mut no_progress(), &cancel)
            .await
            .unwrap(),
        20
    );
    assert!(pending());
    // The rest of this page: nothing left, for now.
    h.engine
        .fetch_bodies(100, &mut no_progress(), &cancel)
        .await
        .unwrap();
    assert!(!pending(), "the fetch caught up with the listing");
    // The next page of headers sets it again.
    run_initial(&h).await;
    assert!(pending());
    h.engine
        .fetch_bodies(500, &mut no_progress(), &cancel)
        .await
        .unwrap();
    assert!(!pending());
    assert_eq!(repo::messages::count_without_body(&store).unwrap(), 0);
}

#[tokio::test]
async fn a_dig_synced_before_the_window_start_was_recorded_derives_it_from_its_oldest_message() {
    // Four hundred days of mail: the first sync lists the window, and a
    // store from before this build has no window start on record.
    let h = harness_with(200, 400, None, options());
    let store = h.stores.dig(&h.dig.id).unwrap();
    assert_eq!(
        window_start_or_derived(&store).unwrap(),
        None,
        "nothing synced"
    );
    run_initial(&h).await;
    let recorded = window_start_or_derived(&store).unwrap().unwrap();
    repo::sync_state::clear(&store, INITIAL_WINDOW_START).unwrap();
    let oldest = repo::messages::oldest_date(&store).unwrap().unwrap();
    assert!(
        oldest >= recorded,
        "the window's oldest message is in the window"
    );
    let derived = window_start_or_derived(&store).unwrap().unwrap();
    assert_eq!(derived, oldest);
    assert_eq!(
        repo::sync_state::get(&store, INITIAL_WINDOW_START)
            .unwrap()
            .as_deref(),
        Some(oldest.to_rfc3339().as_str()),
        "recorded for next time"
    );
    assert!(h.engine.backfill_pending().unwrap());
    // The backfill lists before the derived start and fetches the history.
    let cancel = CancelFlag::new();
    while !h
        .engine
        .backfill_history(&mut no_progress(), &cancel)
        .await
        .unwrap()
        .complete
    {}
    assert_eq!(repo::messages::count(&store).unwrap(), h.listed_since(None));
    assert!(!h.engine.backfill_pending().unwrap());
}

/// The first sync to completion, however many runs it takes.
async fn run_initial(h: &H) {
    let cancel = CancelFlag::new();
    while !h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap()
        .complete
    {}
}

#[tokio::test]
async fn the_history_backfill_extends_the_window_backwards_and_stops_at_the_setting() {
    let now = chrono::Utc::now();
    // Four hundred days of mail, a ninety-day window, history to 200 days.
    let h = harness_with(
        200,
        400,
        Some(200),
        SyncOptions {
            pages_per_run: 2,
            ..options()
        },
    );
    let store = h.stores.dig(&h.dig.id).unwrap();
    run_initial(&h).await;
    let in_window = h.listed_since(Some(now - chrono::Duration::days(90)));
    let to_floor = h.listed_since(Some(now - chrono::Duration::days(200)));
    assert_eq!(repo::messages::count(&store).unwrap(), in_window);
    assert!(
        to_floor > in_window,
        "the fixture has mail before the window"
    );
    assert!(h.engine.backfill_pending().unwrap());
    let cancel = CancelFlag::new();
    let mut steps = Vec::new();
    let mut progress = |p: SyncProgress| {
        if p.phase == SyncPhase::Backfill {
            steps.push(p.step);
        }
    };
    let first = h
        .engine
        .backfill_history(&mut progress, &cancel)
        .await
        .unwrap();
    assert_eq!(first.kind, SyncKind::Backfill);
    assert!(!first.complete, "two pages a run");
    assert_eq!(first.added, 50);
    let oldest_after_first = repo::sync_state::get(&store, BACKFILL_OLDEST)
        .unwrap()
        .unwrap();
    assert!(
        steps.iter().any(|s| s.starts_with("through ")),
        "the step says how far back it has come: {steps:?}"
    );
    // Newest first, so every backfilled message so far has its body.
    assert_eq!(
        repo::messages::count_without_body(&store).unwrap(),
        in_window,
        "the window's bodies come separately; the backfill's come with the page"
    );
    // The rest, each run from a new engine, as after a restart.
    while !h
        .restarted_engine()
        .backfill_history(&mut no_progress(), &cancel)
        .await
        .unwrap()
        .complete
    {}
    assert_eq!(repo::messages::count(&store).unwrap(), to_floor);
    assert!(
        repo::sync_state::get(&store, BACKFILL_OLDEST)
            .unwrap()
            .unwrap()
            < oldest_after_first,
        "it went on backwards"
    );
    assert_eq!(
        repo::sync_state::get(&store, BACKFILL_DONE)
            .unwrap()
            .as_deref(),
        Some("1")
    );
    assert!(!h.engine.backfill_pending().unwrap());
    let again = h
        .engine
        .backfill_history(&mut no_progress(), &cancel)
        .await
        .unwrap();
    assert_eq!((again.added, again.complete), (0, true), "done is done");
    assert!(
        riddle_people::lookup(&h.stores.index(), "priya@example.com")
            .unwrap()
            .unwrap()
            .messages_from
            > 0
    );

    // No setting: the whole mailbox.
    let all = harness_with(200, 400, None, options());
    let store = all.stores.dig(&all.dig.id).unwrap();
    run_initial(&all).await;
    while !all
        .engine
        .backfill_history(&mut no_progress(), &CancelFlag::new())
        .await
        .unwrap()
        .complete
    {}
    assert_eq!(
        repo::messages::count(&store).unwrap(),
        all.listed_since(None)
    );

    // Nothing before the window: done at once, without a listing.
    let short = harness_with(60, 30, None, options());
    run_initial(&short).await;
    let lists = short.fixture.call_count("list");
    let r = short
        .engine
        .backfill_history(&mut no_progress(), &CancelFlag::new())
        .await
        .unwrap();
    assert!(r.complete);
    assert_eq!(
        short.fixture.call_count("list") - lists,
        1,
        "one empty page"
    );
}

#[tokio::test]
async fn a_page_of_label_changes_is_one_write_and_an_unchanged_label_list_is_not_rewritten() {
    let h = harness(60);
    let cancel = CancelFlag::new();
    // Through `sync_once`: the label listing is refreshed on the way in.
    run_initial(&h).await;
    let store = h.stores.dig(&h.dig.id).unwrap();
    let labels_synced_at = || -> i64 {
        store
            .read(|c| {
                c.query_row("SELECT max(synced_at) FROM provider_labels", [], |r| {
                    r.get(0)
                })
            })
            .unwrap()
    };
    let written = labels_synced_at();
    std::thread::sleep(std::time::Duration::from_millis(5));
    // A quiet poll: the label listing is unchanged and not rewritten.
    let before = store.stats().writes;
    let quiet = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!((quiet.kind, quiet.added), (SyncKind::Incremental, 0));
    let quiet_writes = store.stats().writes - before;
    assert_eq!(labels_synced_at(), written, "unchanged listing, no rewrite");
    // Three messages archived from the phone, in however many threads.
    let inbox: Vec<Message> = h
        .fixture
        .snapshot()
        .into_iter()
        .filter(|m| m.labels.iter().any(|l| l == "INBOX") && !m.is_sent)
        .take(3)
        .collect();
    let ids: Vec<String> = inbox.iter().map(|m| m.provider_id.clone()).collect();
    h.fixture
        .modify_labels(&ids, &[], &["INBOX".into()])
        .await
        .unwrap();
    let threads: std::collections::HashSet<&ThreadId> =
        inbox.iter().map(|m| &m.thread_id).collect();
    let before = store.stats().writes;
    let report = h
        .engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert_eq!((report.label_changes, report.updated), (3, 3));
    let writes = store.stats().writes - before;
    assert_eq!(
        writes - quiet_writes,
        1 + threads.len() as u64,
        "one write for the labels and one Thread State per thread touched"
    );
    for m in &inbox {
        assert!(!repo::messages::labels_of(&store, &m.id)
            .unwrap()
            .contains(&"INBOX".to_string()));
    }
    // A changed listing is written.
    std::thread::sleep(std::time::Duration::from_millis(5));
    h.fixture.ensure_label("Receipts").await.unwrap();
    h.engine
        .sync_once(&mut no_progress(), &cancel, &mut |_| {})
        .await
        .unwrap();
    assert!(labels_synced_at() > written);
    assert!(repo::provider_labels::all(&store)
        .unwrap()
        .iter()
        .any(|l| l.name == "Receipts"));
}

#[tokio::test]
async fn refresh_now_asks_for_a_sync_of_the_dig_or_of_every_dig() {
    use riddle_actions::{RuntimeBuilder, Services};
    let stores = Arc::new(Stores::in_memory().unwrap());
    let services = Arc::new(Services::new(
        stores,
        Arc::new(NullSink),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    ));
    let triggers = Arc::new(riddle_mail::RefreshTriggers::new());
    services.insert(triggers.clone());
    let mut b = RuntimeBuilder::new(services);
    for a in crate::actions::actions() {
        b = b.register(a);
    }
    let rt = b.build();
    let dig = DigId::from("dig-x");
    let mut rx = triggers.subscribe();
    let out = rt
        .invoke_as_user("refresh_now", serde_json::json!({"digId": dig}))
        .await
        .unwrap();
    assert_eq!(out.result["heard"], true);
    assert_eq!(rx.try_recv().unwrap(), Some(dig.clone()));
    let all = rt
        .invoke_as_user("refresh_now", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(all.result["heard"], true);
    assert_eq!(rx.try_recv().unwrap(), None, "no Dig named: every account");
}
