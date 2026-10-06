//! The typed fields that replaced client-side guesswork: a Shadow review
//! rides on the message row, an account says what it does to its mailbox,
//! a group names the rules that use it, an outcome carries its own summary,
//! and a demoted Mesh says so.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_shadow_review_list_carries_proposals_and_the_users_marks_on_each_row() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 40, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let mesh = common::install_mesh(
        &app,
        "Mesh: Lists\nROUTES\nIF mailing_list → Spoil",
        MeshStatus::Shadow,
    );
    let all = app
        .list_messages(
            Some(&dig.id),
            &repo::messages::MessageFilter::default(),
            &PageRequest::default(),
        )
        .unwrap();
    let (a, b) = (all.items[0].id.clone(), all.items[1].id.clone());
    for m in [&a, &b] {
        repo::classifications::record(
            &store,
            &Classification {
                id: ClassificationId::new(),
                message_id: m.clone(),
                dig_id: dig.id.clone(),
                importance: Importance::default_for(&Disposition::Spoil),
                disposition: Disposition::Spoil,
                strata: vec![],
                notify: false,
                mechanism: DecisionMechanism::Mesh,
                mesh_id: Some(mesh.id.clone()),
                mesh_version: Some(mesh.version),
                matched_rule: None,
                confidence: None,
                reason: "a list".into(),
                trace_id: TraceId::new(),
                shadow: true,
                created_at: chrono::Utc::now(),
                superseded_at: None,
                series: None,
            },
        )
        .unwrap();
    }
    app.runtime
        .invoke_as_user(
            "review_shadow",
            json!({ "digId": dig.id, "messageId": a, "agree": true }),
        )
        .await
        .unwrap();

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
    assert_eq!(review.items.len(), 2, "only the two proposals");
    assert!(review
        .items
        .iter()
        .all(|r| r.shadow && r.disposition == Some(Disposition::Spoil)));
    let row_a = review.items.iter().find(|r| r.id == a).unwrap();
    let row_b = review.items.iter().find(|r| r.id == b).unwrap();
    assert_eq!(row_a.shadow_review, Some(ShadowReview::Agreed));
    assert_eq!(row_b.shadow_review, None);
    // The plain list still shows real decisions, unmarked as Shadow.
    let plain = app
        .list_messages(
            Some(&dig.id),
            &repo::messages::MessageFilter::default(),
            &PageRequest::default(),
        )
        .unwrap();
    assert!(plain.items.iter().all(|r| !r.shadow));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_account_says_what_sift_does_to_its_mailbox_and_why_it_is_held() {
    let (app, _ops) =
        common::boot_scripted(false, |s| s.autonomy.default = Autonomy::Observe).await;
    let out = app
        .runtime
        .invoke_as_user(
            "add_fixture_dig",
            json!({ "name": "Work", "email": "me@example.com", "seed": 1, "count": 5, "projection": "full" }),
        )
        .await
        .unwrap();
    let id: Dig = serde_json::from_value(out.result).unwrap();
    let dig = app.dig(&id.id).unwrap();
    assert_eq!(
        dig.mailbox,
        Some(MailboxEffect {
            writes: MailboxWrites::Nothing,
            held: Some(HeldReason::Autonomy)
        })
    );
    // At Act in an unarmed dev build, the build is what holds it.
    let settings = app
        .services
        .require::<riddle_settings::SettingsStore>()
        .unwrap();
    let mut s = settings.settings();
    s.autonomy.default = Autonomy::Act;
    settings.save(s).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let dig = app.dig(&id.id).unwrap();
    assert_eq!(
        dig.mailbox.as_ref().and_then(|m| m.held),
        Some(HeldReason::DevUnarmed)
    );
    app.runtime.arm(true);
    let dig = app.dig(&id.id).unwrap();
    assert_eq!(
        dig.mailbox,
        Some(MailboxEffect {
            writes: MailboxWrites::LabelsAndArchive,
            held: None
        })
    );
    assert!(
        !out.summary.is_empty(),
        "outcomes carry their Activity sentence"
    );
    assert!(out.affected.iter().any(|e| e.kind == EntityKind::Dig));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_group_names_the_rules_that_use_it() {
    let (app, _ops) = common::boot_scripted(false, |_| {}).await;
    app.runtime
        .invoke_as_user(
            "save_dig_group",
            json!({ "group": { "name": "Clients", "digs": [] } }),
        )
        .await
        .unwrap();
    app.runtime
        .invoke_as_user(
            "create_mesh",
            json!({ "text": "Mesh: Client mail\nScope: Clients Digs\nROUTES\nIF mailing_list → Spoil", "status": "draft" }),
        )
        .await
        .unwrap();
    let groups = app.list_dig_groups().unwrap();
    let clients = groups.iter().find(|g| g.name == "Clients").unwrap();
    assert_eq!(clients.used_by, vec!["Mesh \"Client mail\""]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_demoted_mesh_records_when_and_why_and_a_user_status_change_clears_it() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 60, json!({})).await;
    let index = app.stores.index();
    let store = app.stores.dig(&dig.id).unwrap();
    let mesh = common::install_mesh(
        &app,
        "Mesh: Eager\nPriority: 60\nROUTES\nIF mailing_list → Spoil",
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
    for m in messages.items.iter().take(24) {
        repo::classifications::record(
            &store,
            &Classification {
                id: ClassificationId::new(),
                message_id: m.id.clone(),
                dig_id: dig.id.clone(),
                importance: Importance::default_for(&Disposition::Spoil),
                disposition: Disposition::Spoil,
                strata: vec![],
                notify: false,
                mechanism: DecisionMechanism::Mesh,
                mesh_id: Some(mesh.id.clone()),
                mesh_version: Some(mesh.version),
                matched_rule: None,
                confidence: None,
                reason: "a list".into(),
                trace_id: TraceId::new(),
                shadow: false,
                created_at: chrono::Utc::now(),
                superseded_at: None,
                series: None,
            },
        )
        .unwrap();
    }
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
    assert_eq!(demoted.status, MeshStatus::Shadow);
    let d = demoted.demoted.expect("the demotion is recorded");
    // The sixth reversal of twenty-four crosses a quarter; the seventh finds
    // the Mesh already in Shadow.
    assert_eq!((d.reversed, d.decided), (6, 24));
    let stats = app.trust_stats(30, Some(&dig.id)).unwrap();
    let rule = stats.rules.iter().find(|r| r.mesh_id == mesh.id).unwrap();
    assert!(rule.reversed_often, "{rule:?}");
    app.runtime
        .invoke_as_user(
            "set_mesh_status",
            json!({ "meshId": mesh.id, "status": "active" }),
        )
        .await
        .unwrap();
    let back = repo::meshes::latest(&index, &mesh.id).unwrap().unwrap();
    assert_eq!(
        back.demoted, None,
        "a status the user chose is not a demotion"
    );
}
