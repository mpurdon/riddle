//! Flow 7 of the UX study: trust is earned on evidence. These prove the
//! evidence is real: a Shadow review counts for the Mesh, a Mesh capped
//! below Act keeps its decisions out of the mailbox, the trust card's
//! numbers come from the stores, and a rule the user keeps reversing steps
//! back to Shadow and says so.

use riddle_app::App;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

async fn booted_with_dig() -> (
    std::sync::Arc<App>,
    std::sync::Arc<riddle_app::ops::Ops>,
    Dig,
) {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "matt@example.com", 5, 60, json!({})).await;
    (app, ops, dig)
}

fn shadow_classification(dig: &DigId, message: &MessageId, mesh: &Mesh) -> Classification {
    Classification {
        id: ClassificationId::new(),
        message_id: message.clone(),
        dig_id: dig.clone(),
        importance: Importance::default_for(&Disposition::Spoil),
        disposition: Disposition::Spoil,
        strata: vec![],
        notify: false,
        mechanism: DecisionMechanism::Mesh,
        mesh_id: Some(mesh.id.clone()),
        mesh_version: Some(mesh.version),
        matched_rule: Some("IF mailing_list → Spoil".into()),
        confidence: None,
        reason: "a mailing list".into(),
        trace_id: TraceId::new(),
        shadow: true,
        created_at: chrono::Utc::now(),
        superseded_at: None,
        series: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reviewing_a_shadow_proposal_counts_for_the_mesh_and_marks_the_row() {
    let (app, _ops, dig) = booted_with_dig().await;
    let index = app.stores.index();
    let store = app.stores.dig(&dig.id).unwrap();
    let mesh = common::install_mesh(
        &app,
        "Mesh: Lists\nROUTES\nIF mailing_list → Spoil",
        MeshStatus::Shadow,
    );
    let messages = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter::default(),
        &PageRequest::default(),
    )
    .unwrap();
    let (a, b) = (messages.items[0].id.clone(), messages.items[1].id.clone());
    for m in [&a, &b] {
        repo::classifications::record(&store, &shadow_classification(&dig.id, m, &mesh)).unwrap();
        riddle_mesh::shadow::record_shadow_proposal(&index, &mesh.id).unwrap();
    }

    app.runtime
        .invoke_as_user(
            "review_shadow",
            json!({ "digId": dig.id, "messageId": a, "agree": true }),
        )
        .await
        .unwrap();
    app.runtime
        .invoke_as_user(
            "review_shadow",
            json!({ "digId": dig.id, "messageId": b, "agree": false, "disposition": { "kind": "filed" } }),
        )
        .await
        .unwrap();

    // The marks ride on the review rows, and a disagreement keeps the
    // proposal on the list beside the user's decision.
    let review = app
        .list_messages(
            Some(&dig.id),
            &repo::messages::MessageFilter {
                shadow_only: true,
                ..Default::default()
            },
            &PageRequest::default(),
        )
        .unwrap();
    let mark = |id: &MessageId| {
        review
            .items
            .iter()
            .find(|r| &r.id == id)
            .expect("the proposal is listed")
            .shadow_review
    };
    assert_eq!(mark(&a), Some(ShadowReview::Agreed));
    assert_eq!(mark(&b), Some(ShadowReview::Disagreed));
    let m = repo::meshes::latest(&index, &mesh.id).unwrap().unwrap();
    assert_eq!((m.shadow_proposed, m.shadow_agreed), (2, 1));
    // Disagreeing recorded the user's decision and a correction against the Mesh.
    let current = repo::classifications::current(&store, &b).unwrap().unwrap();
    assert_eq!(current.disposition, Disposition::Filed);
    assert!(repo::corrections::active(&store, 10)
        .unwrap()
        .iter()
        .any(|c| c.message_id == b && c.mesh_id.as_ref() == Some(&mesh.id)));
    // A message without a proposal cannot be reviewed.
    let none = app
        .runtime
        .invoke_as_user(
            "review_shadow",
            json!({ "digId": dig.id, "messageId": "nope", "agree": true }),
        )
        .await
        .unwrap_err();
    assert_eq!(none.kind, riddle_errors::ErrorKind::Invalid);
}

#[test]
fn a_mesh_can_cap_its_own_autonomy_and_the_cap_survives_an_update_without_the_line() {
    let m = riddle_mesh::parse::parse(
        "Mesh: Promotions v1\nAutonomy: Suggest\nROUTES\nIF mailing_list → Spoil",
    )
    .unwrap();
    assert_eq!(m.autonomy, Some(Autonomy::Suggest));
    assert!(riddle_app::domains::mesh_projection_held(&m).is_some());
    let open =
        riddle_mesh::parse::parse("Mesh: Promotions v1\nROUTES\nIF mailing_list → Spoil").unwrap();
    assert_eq!(open.autonomy, None);
    assert!(riddle_app::domains::mesh_projection_held(&open).is_none());
    let err =
        riddle_mesh::parse::parse("Mesh: P\nAutonomy: Trusted\nROUTES\nIF mailing_list → Spoil")
            .unwrap_err();
    assert!(err.message.contains("Trusted"), "{}", err.message);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_trust_card_counts_decisions_corrections_and_undos_from_the_stores() {
    let (app, _ops, dig) = booted_with_dig().await;
    let before = app.trust_stats(30, Some(&dig.id)).unwrap();
    assert!(before.decisions > 0, "{before:?}");
    assert_eq!(before.corrected, 0);
    assert_eq!(before.undone, 0);
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
    let id = finds.items[0].id.clone();
    let moved = app
        .runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": [id] }),
        )
        .await
        .unwrap();
    let after = app.trust_stats(30, Some(&dig.id)).unwrap();
    assert_eq!(after.corrected, 1, "{after:?}");
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": moved.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    // The test harness invokes without a Dig on the invocation, so the undone
    // entry carries no account; the account-scoped view leaves it out and
    // the all-accounts view counts it.
    let undone = app.trust_stats(30, None).unwrap();
    assert_eq!(undone.undone, 1, "{undone:?}");
    let scoped = app.trust_stats(30, Some(&dig.id)).unwrap();
    assert_eq!(scoped.corrected, 0, "undo retracts the correction");
    assert_eq!(scoped.dig_count, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rule_the_user_keeps_reversing_steps_back_to_shadow_and_says_so() {
    let (app, _ops, dig) = booted_with_dig().await;
    let index = app.stores.index();
    let store = app.stores.dig(&dig.id).unwrap();
    let mesh = common::install_mesh(
        &app,
        "Mesh: Eager\nROUTES\nIF mailing_list → Spoil",
        MeshStatus::Active,
    );
    let messages = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter::default(),
        &PageRequest {
            cursor: None,
            limit: Some(40),
        },
    )
    .unwrap();
    assert!(
        messages.items.len() >= 24,
        "{} messages",
        messages.items.len()
    );
    // Twenty-four decisions by the Mesh...
    for m in messages.items.iter().take(24) {
        let mut k = shadow_classification(&dig.id, &m.id, &mesh);
        k.shadow = false;
        repo::classifications::record(&store, &k).unwrap();
    }
    let mut events = app.events.subscribe();
    // ...six of which the user reverses: the seventh reversal crosses a quarter.
    for m in messages.items.iter().take(7) {
        app.runtime
            .invoke_as_user(
                "set_disposition",
                json!({ "digId": dig.id, "messageId": m.id, "disposition": { "kind": "filed" } }),
            )
            .await
            .unwrap();
    }
    let demoted = repo::meshes::latest(&index, &mesh.id).unwrap().unwrap();
    assert_eq!(demoted.status, MeshStatus::Shadow, "{demoted:?}");
    let mut told = false;
    while let Ok(e) = events.try_recv() {
        if let RiddleEvent::Notify { title, .. } = e {
            if title == "A rule stepped back" {
                told = true;
            }
        }
    }
    assert!(told, "the user is told when a rule is demoted");
}
