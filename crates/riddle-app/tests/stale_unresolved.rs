//! Unresolved mail decided under rules that have changed since goes back to
//! Sift on its next pass; mail the user decided is never re-decided. 27 of
//! 91 Unresolved on a real mailbox predated its rule packs and nothing ever
//! looked at them again (13 Oct 2026).

use riddle_core::*;
use riddle_store::repo;
use riddle_sync::engine::CancelFlag;
use serde_json::json;

mod common;

fn unresolved(
    id: &MessageId,
    dig: &DigId,
    mechanism: DecisionMechanism,
    days_ago: i64,
) -> Classification {
    Classification {
        id: ClassificationId::new(),
        message_id: id.clone(),
        dig_id: dig.clone(),
        importance: Importance::default_for(&Disposition::Unresolved),
        disposition: Disposition::Unresolved,
        strata: vec![],
        notify: false,
        mechanism,
        mesh_id: None,
        mesh_version: None,
        matched_rule: None,
        confidence: None,
        reason: "Unresolved: no route matched.".into(),
        trace_id: TraceId::new(),
        shadow: false,
        created_at: chrono::Utc::now() - chrono::Duration::days(days_ago),
        superseded_at: None,
        series: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unresolved_decided_before_a_rule_change_is_decided_again_and_yours_is_kept() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 9, 6, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let ids = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        10,
    )
    .unwrap();
    let (stale, yours) = (ids[0].clone(), ids[1].clone());
    repo::classifications::record(
        &store,
        &unresolved(&stale, &dig.id, DecisionMechanism::Llm, 3),
    )
    .unwrap();
    repo::classifications::record(
        &store,
        &unresolved(&yours, &dig.id, DecisionMechanism::User, 3),
    )
    .unwrap();

    // A rule change after those decisions.
    app.runtime
        .invoke_as_user(
            "save_list",
            json!({ "name": "test_list", "entries": ["example.com"] }),
        )
        .await
        .unwrap();
    app.full_pass(&dig.id, 500, &CancelFlag::new())
        .await
        .unwrap();

    let again = repo::classifications::current(&store, &stale)
        .unwrap()
        .unwrap();
    assert!(
        again.created_at > chrono::Utc::now() - chrono::Duration::minutes(5),
        "decided again"
    );
    let kept = repo::classifications::current(&store, &yours)
        .unwrap()
        .unwrap();
    assert_eq!(
        kept.mechanism,
        DecisionMechanism::User,
        "your decision stands"
    );
    let acts = app
        .list_activity(
            &repo::activities::ActivityFilter {
                actor: Some(Actor::Sift),
                action: Some("decide_again".into()),
                ..Default::default()
            },
            &PageRequest::default(),
        )
        .unwrap();
    assert!(!acts.items.is_empty(), "the ledger says Sift did it");

    // Decided again after the change, it is not picked up a second time.
    let before = again.id;
    app.full_pass(&dig.id, 500, &CancelFlag::new())
        .await
        .unwrap();
    assert_eq!(
        repo::classifications::current(&store, &stale)
            .unwrap()
            .unwrap()
            .id,
        before
    );
}
