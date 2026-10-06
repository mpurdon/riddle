use crate::engine::{CancelFlag, SyncEngine, SyncKind, SyncOptions, SyncPhase, SyncProgress};
use crate::poller::{run_poller, PollerConfig, PollerHooks};
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

fn harness(count: u32) -> H {
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
            ..Default::default()
        },
    ));
    let registry = Arc::new(ProviderRegistry::new(stores.clone()));
    registry.insert_fixture(dig.id.clone(), fixture.clone());
    let events = Arc::new(RecordingSink::default());
    let engine = Arc::new(
        SyncEngine::new(dig.clone(), fixture.clone(), stores.clone(), events.clone()).with_options(
            SyncOptions {
                page_size: 25,
                window_days: Some(90),
                body_batch: 20,
                ..Default::default()
            },
        ),
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
async fn the_poller_syncs_on_a_trigger_reports_status_and_stops_when_cancelled() {
    let h = harness(40);
    let trigger = Arc::new(tokio::sync::Notify::new());
    let cancel = CancelFlag::new();
    let reports = Arc::new(Mutex::new(0u32));
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let (r2, s2) = (reports.clone(), statuses.clone());
    let hooks = PollerHooks {
        on_report: Some(Box::new(move |_| *r2.lock().unwrap() += 1)),
        on_status: Some(Box::new(move |s| s2.lock().unwrap().push(s))),
        ..Default::default()
    };
    let config = PollerConfig {
        interval: std::time::Duration::from_secs(3600),
        bodies_per_cycle: 500,
        ..Default::default()
    };
    let handle = tokio::spawn(run_poller(
        h.engine.clone(),
        trigger.clone(),
        cancel.clone(),
        config,
        hooks,
    ));
    // The first pass runs on start; then a trigger runs another.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while *reports.lock().unwrap() < 1 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("first pass");
    h.fixture.inject("custom", None, None, None).unwrap();
    trigger.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while *reports.lock().unwrap() < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("triggered pass");
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), handle)
        .await
        .expect("poller exits")
        .unwrap();
    let store = h.stores.dig(&h.dig.id).unwrap();
    assert!(repo::messages::count(&store).unwrap() >= 40);
    assert!(statuses
        .lock()
        .unwrap()
        .iter()
        .any(|s| matches!(s, DigStatus::Synced)));
    assert_eq!(
        repo::messages::count_without_body(&store).unwrap(),
        0,
        "bodies caught up within the cycles"
    );
}

#[tokio::test]
async fn refresh_now_wakes_the_dig_poller() {
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
