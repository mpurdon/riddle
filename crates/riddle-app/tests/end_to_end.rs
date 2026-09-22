//! The whole Riddle on a fixture Dig, in memory, with the scripted judge:
//! add a Dig, sync it, sift it, see Finds and a summary, explain a decision,
//! move a message to Spoil through the Action layer and undo it.

use riddle_app::domains::JudgeConfig;
use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_store::repo;
use riddle_sync::engine::CancelFlag;
use serde_json::json;
use std::sync::Arc;

async fn app() -> Arc<App> {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "test",
    )))
    .unwrap();
    app.attach_domains(None).unwrap();
    app.attach_judge(&JudgeConfig {
        provider: "scripted".into(),
        ..Default::default()
    })
    .await;
    // A dev build starts unarmed (§59); the tests act as the user arming it.
    app.runtime.arm(true);
    app
}

async fn add_dig(app: &Arc<App>, name: &str, count: u32) -> Dig {
    let out = app
        .runtime
        .invoke_as_user(
            "add_fixture_dig",
            json!({ "name": name, "email": "matt@example.com", "seed": 7, "count": count }),
        )
        .await
        .unwrap();
    serde_json::from_value(out.result).unwrap()
}

#[tokio::test]
async fn a_fixture_dig_syncs_sifts_and_produces_finds_and_an_honest_summary() {
    let app = app().await;
    let dig = add_dig(&app, "Work", 120).await;
    let cancel = CancelFlag::new();
    let report = app.full_pass(&dig.id, 500, &cancel).await.unwrap();
    let sync = report.sync.as_ref().expect("sync ran");
    assert!(sync.added >= 100, "synced {} messages", sync.added);
    assert_eq!(report.sift.remaining, 0, "backlog drained");
    assert!(report.sift.processed >= 100);
    assert!(!report.sift.judge_unavailable);

    let store = app.stores.dig(&dig.id).unwrap();
    let counts = repo::classifications::counts(&store, None, false).unwrap();
    let total: u64 = counts.values().sum();
    assert_eq!(total as u64, report.sift.processed);
    // Every message has exactly one disposition; the fixture's populations
    // produce all five.
    assert!(
        counts.get(&DispositionKind::Find).copied().unwrap_or(0) > 0,
        "finds: {counts:?}"
    );
    assert!(
        counts.get(&DispositionKind::Spoil).copied().unwrap_or(0) > 0,
        "spoil: {counts:?}"
    );
    assert!(
        counts.get(&DispositionKind::Filed).copied().unwrap_or(0) > 0,
        "filed: {counts:?}"
    );
    assert!(
        counts
            .get(&DispositionKind::Unresolved)
            .copied()
            .unwrap_or(0)
            > 0,
        "unresolved is a real state: {counts:?}"
    );

    // A phishing lookalike becomes Contamination on deterministic evidence,
    // without a judge call (§18, §58).
    app.runtime
        .invoke_as_user(
            "inject_fixture_message",
            json!({ "digId": dig.id, "template": "phishing" }),
        )
        .await
        .unwrap();
    let r2 = app.full_pass(&dig.id, 500, &cancel).await.unwrap();
    assert_eq!(r2.sift.processed, 1);
    let counts = repo::classifications::counts(&store, None, false).unwrap();
    assert_eq!(
        counts
            .get(&DispositionKind::Contamination)
            .copied()
            .unwrap_or(0),
        1,
        "contamination: {counts:?}"
    );
    let contaminated = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter {
            disposition: Some("contamination".into()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    let why = app.explain(&dig.id, &contaminated.items[0].id).unwrap();
    assert_eq!(why.mechanism, DecisionMechanism::Gate);
    assert!(why.signals.is_empty(), "no judge call for Contamination");

    let summary = app
        .summary(chrono::Utc::now() - chrono::Duration::days(90))
        .unwrap();
    assert_eq!(summary.dig_count, 1);
    assert!(summary.caught_up);
    assert!(summary.finds_open > 0);
    assert_eq!(
        summary.finds_open,
        repo::finds::count_open(&app.stores.index()).unwrap()
    );

    // Every classification carries a trace with facts and a build stamp (§92).
    let traces = app.recent_traces(Some(&dig.id), 5).unwrap();
    assert_eq!(traces.len(), 5);
    assert!(traces
        .iter()
        .all(|t| t.build_stamp.contains("test") || !t.build_stamp.is_empty()));

    // "Why?" resolves from the stored trace.
    let m = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter {
            disposition: Some("spoil".into()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    let first = &m.items[0];
    let why = app.explain(&dig.id, &first.id).unwrap();
    assert_eq!(why.disposition, Disposition::Spoil);
    assert!(
        why.mesh_name.is_some() || why.gate.is_some(),
        "an explanation names the Mesh or the gate"
    );
}

#[tokio::test]
async fn move_to_spoil_records_activity_and_undo_restores_the_classification() {
    let app = app().await;
    let dig = add_dig(&app, "Personal", 60).await;
    let cancel = CancelFlag::new();
    app.full_pass(&dig.id, 500, &cancel).await.unwrap();
    let store = app.stores.dig(&dig.id).unwrap();
    let finds = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter {
            disposition: Some("find".into()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    let target = finds.items.first().expect("a Find message").id.clone();

    let out = app
        .runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": [target] }),
        )
        .await
        .unwrap();
    let activity_id = out.activity_id.expect("mutation recorded");
    assert_eq!(
        repo::classifications::current(&store, &target)
            .unwrap()
            .unwrap()
            .disposition,
        Disposition::Spoil
    );
    assert!(
        !repo::corrections::for_message(&store, &target)
            .unwrap()
            .is_empty(),
        "a correction was recorded"
    );
    let a = repo::activities::get(&app.stores.index(), &activity_id)
        .unwrap()
        .unwrap();
    assert!(matches!(a.undo, UndoInfo::Undoable { .. }));

    // Undo through the Action's own inverse.
    let action = app.runtime.action("move_to_spoil").unwrap();
    let restore = match a.undo {
        UndoInfo::Undoable { restore, .. } => restore,
        _ => unreachable!(),
    };
    let ctx = riddle_actions::ActionContext {
        services: app.services.clone(),
        principal: Principal::User,
        dig_id: Some(dig.id.clone()),
        idempotency_key: None,
        activity_id: ActivityId::new(),
    };
    action.undo(&ctx, restore).await.unwrap();
    let back = repo::classifications::current(&store, &target)
        .unwrap()
        .unwrap();
    assert!(
        matches!(back.disposition, Disposition::Find { .. }),
        "restored to a Find, got {:?}",
        back.disposition
    );
}

#[tokio::test]
async fn an_injected_review_request_arrives_and_becomes_a_review_find() {
    let app = app().await;
    let dig = add_dig(&app, "Work", 40).await;
    let cancel = CancelFlag::new();
    app.full_pass(&dig.id, 500, &cancel).await.unwrap();
    let before = repo::finds::count_open(&app.stores.index()).unwrap();
    app.runtime
        .invoke_as_user(
            "inject_fixture_message",
            json!({ "digId": dig.id, "template": "github_review" }),
        )
        .await
        .unwrap();
    let report = app.full_pass(&dig.id, 500, &cancel).await.unwrap();
    assert_eq!(report.sync.as_ref().unwrap().added, 1);
    assert_eq!(report.sift.processed, 1);
    let after = repo::finds::count_open(&app.stores.index()).unwrap();
    assert_eq!(after, before + 1, "one new Find");
    let open = repo::finds::list(
        &app.stores.index(),
        &repo::finds::FindFilter {
            status: Some(FindStatus::Open),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    assert!(
        open.items.iter().any(|f| f.find_type == FindType::Review),
        "a Review Find exists"
    );
}

#[tokio::test]
async fn without_a_judge_nothing_is_hidden_by_a_semantic_route() {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "test",
    )))
    .unwrap();
    app.attach_domains(None).unwrap();
    // no judge attached
    let dig = add_dig(&app, "Work", 40).await;
    let cancel = CancelFlag::new();
    let report = app.full_pass(&dig.id, 500, &cancel).await.unwrap();
    assert!(report.sift.judge_unavailable);
    let store = app.stores.dig(&dig.id).unwrap();
    let counts = repo::classifications::counts(&store, None, false).unwrap();
    // Spoil requires Signals; only gates (Contamination) and fact-only routes may hide.
    assert_eq!(
        counts.get(&DispositionKind::Spoil).copied().unwrap_or(0),
        0,
        "{counts:?}"
    );
    assert!(
        counts
            .get(&DispositionKind::Unresolved)
            .copied()
            .unwrap_or(0)
            > 0
    );
}
