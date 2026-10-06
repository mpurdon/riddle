//! Sifting again: Riddle's decisions are retired and made anew with the
//! current rules, and the user's own decisions stay where they are.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::time::Duration;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resift_decides_every_message_again_but_keeps_the_users_decisions() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 77, 12, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    assert_eq!(repo::messages::unclassified_count(&store).unwrap(), 0);
    let total = repo::messages::count(&store).unwrap() as usize;
    let all = app
        .list_messages(
            Some(&dig.id),
            &repo::messages::MessageFilter::default(),
            &PageRequest::default(),
        )
        .unwrap();
    let mine = all.items[0].id.clone();
    app.runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": mine, "disposition": { "kind": "filed" } }),
        )
        .await
        .unwrap();
    let before: Vec<(MessageId, ClassificationId)> = all
        .items
        .iter()
        .map(|m| {
            (
                m.id.clone(),
                repo::classifications::current(&store, &m.id)
                    .unwrap()
                    .unwrap()
                    .id,
            )
        })
        .collect();

    let out = app
        .runtime
        .invoke_as_user("resift", json!({ "digId": dig.id }))
        .await
        .unwrap();
    assert_eq!(
        out.result["messages"],
        json!(total - 1),
        "every decision but the user's is retired"
    );
    assert!(out.summary.starts_with("Sifting "), "{}", out.summary);
    ops.runner.run_until_idle(Duration::from_secs(60)).await;

    assert_eq!(
        repo::messages::unclassified_count(&store).unwrap(),
        0,
        "every message is decided again"
    );
    for (id, old) in before {
        let now = repo::classifications::current(&store, &id)
            .unwrap()
            .unwrap();
        if id == mine {
            assert_eq!(now.id, old, "the user's decision is kept");
            assert_eq!(now.mechanism, DecisionMechanism::User);
        } else {
            assert_ne!(now.id, old, "a fresh decision for {id}");
            assert_ne!(now.mechanism, DecisionMechanism::User);
        }
    }
}
