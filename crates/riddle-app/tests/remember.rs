//! "Always do this for this sender": a message the user decides (here a
//! charity appeal made a Know Find under Charity) becomes a rule for its
//! sender in Your corrections, and the next message from that sender is
//! decided the same way without asking. Corrections used to teach only
//! "never Spoil".

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::time::Duration;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_decision_remembered_for_a_sender_decides_their_next_message() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    // The fixture provider stands in for Gmail; corrections write to it.
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 5, 5, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let sync = || async {
        ops.queue
            .enqueue(
                riddle_jobs::JobSpec::new(JobKind::IncrementalSync, "Sync").dig(dig.id.clone()),
            )
            .unwrap();
        ops.runner.run_until_idle(Duration::from_secs(60)).await;
    };
    let inject = |subject: &'static str| {
        let app = app.clone();
        let dig = dig.id.clone();
        async move {
            app.runtime
                .invoke_as_user(
                    "inject_fixture_message",
                    json!({ "digId": dig, "from": "appeals@hospital-foundation.example", "subject": subject, "body": "Your gift this fall funds new equipment." }),
                )
                .await
                .unwrap();
        }
    };
    inject("Help us reach our fall goal").await;
    sync().await;
    let from_sender = |store: &riddle_store::Store| {
        let mut ids: Vec<(chrono::DateTime<chrono::Utc>, MessageId)> = repo::messages::ids_since(
            store,
            &(chrono::Utc::now() - chrono::Duration::days(3650)),
            10_000,
        )
        .unwrap()
        .into_iter()
        .filter_map(|id| {
            let m = repo::messages::get(store, &dig.id, &id).unwrap()?;
            (m.from.email == "appeals@hospital-foundation.example").then_some((m.date, id))
        })
        .collect();
        ids.sort();
        ids.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    };
    let first = from_sender(&store).pop().expect("the appeal arrived");

    // The user decides it: a Know Find, labelled Charity.
    app.runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": first, "disposition": { "kind": "find", "find_type": "know" } }),
        )
        .await
        .unwrap();
    app.runtime
        .invoke_as_user(
            "label_messages",
            json!({ "digId": dig.id, "messageIds": [first], "strata": ["Charity"] }),
        )
        .await
        .unwrap();
    let out = app
        .runtime
        .invoke_as_user(
            "remember_for_sender",
            json!({ "digId": dig.id, "messageId": first, "scope": "sender" }),
        )
        .await
        .unwrap();
    assert!(
        out.summary.contains("appeals@hospital-foundation.example"),
        "{}",
        out.summary
    );
    let mesh = repo::meshes::find_by_name(&app.stores.index(), "Your corrections")
        .unwrap()
        .expect("Your corrections exists");
    assert_eq!(
        mesh.status,
        MeshStatus::Active,
        "a stated rule works at once"
    );
    assert_eq!(mesh.priority, -90);

    // The next appeal is decided by the rule.
    inject("A match on every gift this week").await;
    sync().await;
    let next = from_sender(&store).pop().unwrap();
    assert_ne!(next, first);
    let k = repo::classifications::current(&store, &next)
        .unwrap()
        .unwrap();
    assert_eq!(
        k.disposition,
        Disposition::Find {
            find_type: FindType::Know
        }
    );
    assert_eq!(k.strata, vec!["Charity".to_string()]);
    assert_eq!(k.mesh_id.as_ref(), Some(&mesh.id));

    // A domain rule joins it; the Mesh stays Active.
    let dom = app
        .runtime
        .invoke_as_user(
            "remember_for_sender",
            json!({ "digId": dig.id, "messageId": first, "scope": "domain" }),
        )
        .await
        .unwrap();
    let mesh = repo::meshes::find_by_name(&app.stores.index(), "Your corrections")
        .unwrap()
        .unwrap();
    assert_eq!((mesh.status, mesh.routes.len()), (MeshStatus::Active, 2));

    // Undo forgets the domain rule and keeps the sender's.
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": dom.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    let mesh = repo::meshes::find_by_name(&app.stores.index(), "Your corrections")
        .unwrap()
        .unwrap();
    assert_eq!(mesh.routes.len(), 1);
    assert_eq!(mesh.status, MeshStatus::Active);
}
