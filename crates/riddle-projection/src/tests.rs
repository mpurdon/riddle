use crate::outbox::{enqueue_classification, undo_projection, OutboxProcessor};
use crate::plan::SPOIL_LABEL;
use riddle_actions::{Runtime, RuntimeBuilder, Services};
use riddle_core::*;
use riddle_mail::{Detail, FixtureProvider, FixtureSpec, MailProvider, ProviderRegistry};
use riddle_store::{repo, Stores};
use serde_json::json;
use std::sync::Arc;

struct Harness {
    rt: Arc<Runtime>,
    dig: Dig,
    registry: Arc<ProviderRegistry>,
    fixture: Arc<FixtureProvider>,
    processor: OutboxProcessor,
}

fn harness(mode: ProjectionMode) -> Harness {
    let stores = Arc::new(Stores::in_memory().unwrap());
    let services = Arc::new(Services::new(
        stores.clone(),
        Arc::new(RecordingSink::default()),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    ));
    let registry = Arc::new(ProviderRegistry::new(stores.clone()));
    services.insert(registry.clone());
    let mut b = RuntimeBuilder::new(services.clone());
    for a in crate::actions::actions() {
        b = b.register(a);
    }
    let rt = b.build();
    let dig = Dig {
        id: DigId::from("dig-a"),
        name: "Work".into(),
        email: "matt@example.com".into(),
        provider: ProviderKind::Fixture,
        group: None,
        privacy: PrivacyMode::CloudJudgment,
        projection: mode,
        status: DigStatus::NotConnected,
        shadow_started_at: None,
        created_at: chrono::Utc::now(),
        disabled: false,
        history_days: None,
        mailbox: None,
    };
    repo::digs::upsert(&stores.index(), &dig).unwrap();
    let fixture = Arc::new(FixtureProvider::new(
        dig.id.clone(),
        FixtureSpec {
            count: 80,
            seed: 11,
            ..Default::default()
        },
    ));
    registry.insert_fixture(dig.id.clone(), fixture.clone());
    // Bring the fixture into the store as sync would (metadata only).
    let store = stores.dig(&dig.id).unwrap();
    let all = fixture.snapshot();
    store
        .write(|c| {
            for m in &all {
                repo::messages::upsert_in(
                    c,
                    &Message {
                        body_text: None,
                        body_html: None,
                        has_body: false,
                        ..m.clone()
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let processor = OutboxProcessor::new(stores, registry.clone());
    Harness {
        rt,
        dig,
        registry,
        fixture,
        processor,
    }
}

fn inbox_message(h: &Harness) -> Message {
    h.fixture
        .snapshot()
        .into_iter()
        .find(|m| m.labels.iter().any(|l| l == "INBOX") && !m.is_sent)
        .unwrap()
}

fn sift_classification(dig: &Dig, m: &Message, d: Disposition) -> Classification {
    Classification {
        id: ClassificationId::new(),
        message_id: m.id.clone(),
        dig_id: dig.id.clone(),
        importance: Importance::default_for(&d),
        disposition: d,
        strata: vec![],
        notify: false,
        mechanism: DecisionMechanism::Mesh,
        mesh_id: None,
        mesh_version: None,
        matched_rule: None,
        confidence: Some(0.9),
        reason: "test".into(),
        trace_id: TraceId::new(),
        shadow: false,
        created_at: chrono::Utc::now(),
        superseded_at: None,
        series: None,
    }
}

#[tokio::test]
async fn a_spoil_classification_is_projected_as_archive_plus_label_and_undo_restores_it() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    let k = sift_classification(&h.dig, &m, Disposition::Spoil);
    repo::classifications::record(&store, &k).unwrap();
    let row = enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata).unwrap();
    assert!(row.is_some());
    // Same plan again is one row (idempotency key).
    assert_eq!(repo::outbox::pending_count(&store).unwrap(), 1);
    enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata).unwrap();
    assert_eq!(repo::outbox::pending_count(&store).unwrap(), 1);

    let report = h.processor.process(&h.dig, 100).await.unwrap();
    assert_eq!(report.applied, 1);
    assert!(report.conflicts.is_empty());
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(labels.contains(&SPOIL_LABEL.to_string()));
    assert!(!labels.contains(&"INBOX".to_string()));
    assert_eq!(repo::messages::labels_of(&store, &m.id).unwrap(), labels);
    assert_eq!(repo::outbox::pending_count(&store).unwrap(), 0);

    undo_projection(&store, &h.dig, &m, true, "t").unwrap();
    let report = h.processor.process(&h.dig, 100).await.unwrap();
    assert_eq!(report.applied, 1);
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(!labels.contains(&SPOIL_LABEL.to_string()));
    assert!(labels.contains(&"INBOX".to_string()));
    assert_eq!(
        h.fixture.call_count("ensure_label"),
        1,
        "the label was created once and cached"
    );
}

#[tokio::test]
async fn a_provider_side_change_underneath_a_queued_projection_wins_and_is_reported() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    let k = sift_classification(&h.dig, &m, Disposition::Spoil);
    repo::classifications::record(&store, &k).unwrap();
    enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata).unwrap();
    // A human moves it to trash from another client before projection runs.
    h.fixture
        .modify_labels(
            std::slice::from_ref(&m.provider_id),
            &["TRASH".into()],
            &["INBOX".into()],
        )
        .await
        .unwrap();
    let report = h.processor.process(&h.dig, 100).await.unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].message_id, m.id);
    assert!(report.conflicts[0].actual.contains(&"TRASH".to_string()));
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(
        !labels.contains(&SPOIL_LABEL.to_string()),
        "the provider's state was not overwritten"
    );
    assert_eq!(
        repo::messages::labels_of(&store, &m.id).unwrap(),
        labels,
        "the store adopted the provider's state"
    );
    assert_eq!(repo::outbox::pending_count(&store).unwrap(), 0);
    assert_eq!(repo::outbox::recent_errors(&store, 5).unwrap().len(), 1);
}

#[tokio::test]
async fn a_read_only_volatile_change_is_not_a_conflict() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    let k = sift_classification(&h.dig, &m, Disposition::Spoil);
    repo::classifications::record(&store, &k).unwrap();
    enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata).unwrap();
    h.fixture
        .modify_labels(
            std::slice::from_ref(&m.provider_id),
            &[],
            &["UNREAD".into()],
        )
        .await
        .unwrap();
    let report = h.processor.process(&h.dig, 100).await.unwrap();
    assert_eq!(report.applied, 1);
    assert!(report.conflicts.is_empty());
}

#[tokio::test]
async fn rows_in_one_thread_apply_in_order_across_passes() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    // First spoil it, then (a later decision) restore it: the second row must
    // not be applied before the first.
    let k = sift_classification(&h.dig, &m, Disposition::Spoil);
    repo::classifications::record(&store, &k).unwrap();
    enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata).unwrap();
    let p = crate::plan::ProjectionPlan {
        add: vec!["INBOX".into()],
        remove: vec![SPOIL_LABEL.into()],
    };
    crate::outbox::enqueue(&store, &m, &p, "later", Some("second")).unwrap();
    let first = h.processor.process(&h.dig, 100).await.unwrap();
    assert_eq!(first.applied, 1);
    assert!(h
        .fixture
        .labels_of(&m.provider_id)
        .unwrap()
        .contains(&SPOIL_LABEL.to_string()));
    let second = h.processor.process(&h.dig, 100).await.unwrap();
    assert_eq!(second.applied, 1);
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(labels.contains(&"INBOX".to_string()) && !labels.contains(&SPOIL_LABEL.to_string()));
}

#[tokio::test]
async fn projection_mode_none_writes_nothing_to_the_provider() {
    let h = harness(ProjectionMode::None);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    let k = sift_classification(&h.dig, &m, Disposition::Spoil);
    assert!(
        enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata)
            .unwrap()
            .is_none()
    );
    assert_eq!(repo::outbox::pending_count(&store).unwrap(), 0);
}

#[tokio::test]
async fn archive_then_undo_through_the_action_layer_round_trips_store_and_provider() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    let out =
        h.rt.invoke_as_user(
            "archive_messages",
            json!({"digId": h.dig.id, "messageIds": [m.id]}),
        )
        .await
        .unwrap();
    assert_eq!(out.result["archived"], 1);
    let k = repo::classifications::current(&store, &m.id)
        .unwrap()
        .unwrap();
    assert_eq!(k.disposition, Disposition::Filed);
    assert_eq!(k.mechanism, DecisionMechanism::User);
    assert!(
        repo::traces::get(&store, &k.trace_id).unwrap().is_some(),
        "a user decision still has a trace"
    );
    h.processor.drain(&h.dig, 5).await.unwrap();
    assert!(!h
        .fixture
        .labels_of(&m.provider_id)
        .unwrap()
        .contains(&"INBOX".to_string()));
    assert!(!repo::messages::labels_of(&store, &m.id)
        .unwrap()
        .contains(&"INBOX".to_string()));

    let activity = repo::activities::get(
        &h.rt.services.stores.index(),
        out.activity_id.as_ref().unwrap(),
    )
    .unwrap()
    .unwrap();
    let UndoInfo::Undoable { restore, .. } = activity.undo else {
        panic!("archive is undoable")
    };
    let action = h.rt.action("archive_messages").unwrap();
    let ctx = riddle_actions::ActionContext {
        services: h.rt.services.clone(),
        principal: Principal::User,
        dig_id: Some(h.dig.id.clone()),
        idempotency_key: None,
        activity_id: ActivityId::new(),
    };
    let undone = action.undo(&ctx, restore).await.unwrap();
    assert_eq!(undone.value["restored"], 1);
    assert_eq!(
        repo::classifications::current(&store, &m.id)
            .unwrap()
            .unwrap()
            .disposition,
        Disposition::Unresolved
    );
    h.processor.drain(&h.dig, 5).await.unwrap();
    assert!(repo::messages::labels_of(&store, &m.id)
        .unwrap()
        .contains(&"INBOX".to_string()));
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(labels.contains(&"INBOX".to_string()));
    assert!(!labels.iter().any(|l| l.starts_with("Riddle/")));
}

#[tokio::test]
async fn restoring_from_spoil_records_a_correction_that_undo_retracts() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    let k = sift_classification(&h.dig, &m, Disposition::Spoil);
    repo::classifications::record(&store, &k).unwrap();
    enqueue_classification(&store, &h.dig, &m, &k.disposition, &k.strata).unwrap();
    h.processor.drain(&h.dig, 5).await.unwrap();
    let out =
        h.rt.invoke_as_user(
            "restore_messages",
            json!({"digId": h.dig.id, "messageIds": [m.id]}),
        )
        .await
        .unwrap();
    assert_eq!(out.result["restored"], 1);
    assert_eq!(
        repo::classifications::current(&store, &m.id)
            .unwrap()
            .unwrap()
            .disposition,
        Disposition::Unresolved
    );
    let corrections = repo::corrections::for_message(&store, &m.id).unwrap();
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].kind, CorrectionKind::Restore);
    assert_eq!(corrections[0].from_disposition, Some(Disposition::Spoil));
    assert_eq!(corrections[0].activity_id, out.activity_id);
    h.processor.drain(&h.dig, 5).await.unwrap();
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(labels.contains(&"INBOX".to_string()) && !labels.contains(&SPOIL_LABEL.to_string()));

    let activity = repo::activities::get(
        &h.rt.services.stores.index(),
        out.activity_id.as_ref().unwrap(),
    )
    .unwrap()
    .unwrap();
    let UndoInfo::Undoable { restore, .. } = activity.undo else {
        panic!()
    };
    let ctx = riddle_actions::ActionContext {
        services: h.rt.services.clone(),
        principal: Principal::User,
        dig_id: Some(h.dig.id.clone()),
        idempotency_key: None,
        activity_id: ActivityId::new(),
    };
    h.rt.action("restore_messages")
        .unwrap()
        .undo(&ctx, restore)
        .await
        .unwrap();
    assert!(
        repo::corrections::for_message(&store, &m.id).unwrap()[0]
            .retracted_at
            .is_some(),
        "undone corrections teach nothing"
    );
    assert_eq!(
        repo::classifications::current(&store, &m.id)
            .unwrap()
            .unwrap()
            .disposition,
        Disposition::Spoil
    );
    h.processor.drain(&h.dig, 5).await.unwrap();
    assert!(h
        .fixture
        .labels_of(&m.provider_id)
        .unwrap()
        .contains(&SPOIL_LABEL.to_string()));
}

#[tokio::test]
async fn labelling_adds_strata_and_projects_them_and_snooze_is_riddle_only() {
    let h = harness(ProjectionMode::Full);
    let store = h.rt.services.stores.dig(&h.dig.id).unwrap();
    let m = inbox_message(&h);
    h.rt.invoke_as_user(
        "label_messages",
        json!({"digId": h.dig.id, "messageIds": [m.id], "strata": ["Finance", "Projects"]}),
    )
    .await
    .unwrap();
    let counts = repo::messages::stratum_counts(&store).unwrap();
    assert!(counts.iter().any(|(s, n)| s == "Finance" && *n == 1));
    h.processor.drain(&h.dig, 5).await.unwrap();
    let labels = h.fixture.labels_of(&m.provider_id).unwrap();
    assert!(
        labels.contains(&"Riddle/Finance".to_string())
            && labels.contains(&"Riddle/Projects".to_string())
    );
    assert!(
        labels.contains(&"INBOX".to_string()),
        "strata never archive"
    );

    let until = chrono::Utc::now() + chrono::Duration::hours(3);
    let calls_before = h.fixture.call_count("modify_labels");
    let out =
        h.rt.invoke_as_user(
            "snooze_messages",
            json!({"digId": h.dig.id, "messageIds": [m.id], "until": until}),
        )
        .await
        .unwrap();
    assert_eq!(out.result["snoozed"], 1);
    assert!(repo::drafts::snoozed_until(&store, m.id.as_str())
        .unwrap()
        .is_some());
    h.processor.drain(&h.dig, 5).await.unwrap();
    assert_eq!(
        h.fixture.call_count("modify_labels"),
        calls_before,
        "snooze is Riddle-only"
    );
    let past = h.rt.invoke_as_user("snooze_messages", json!({"digId": h.dig.id, "messageIds": [m.id], "until": chrono::Utc::now() - chrono::Duration::hours(1)})).await.unwrap_err();
    assert_eq!(past.kind, riddle_errors::ErrorKind::Invalid);
    assert!(h.registry.get(&h.dig.id).is_some());
    let _ = h
        .fixture
        .get_message(&m.provider_id, Detail::Metadata)
        .await
        .unwrap();
}
