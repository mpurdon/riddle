//! Deciding one message again: its decision is retired, its open Find closes
//! as Superseded, Sift decides it afresh, and undo puts the earlier decision
//! and the Find back.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::time::Duration;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_message_is_decided_again_and_undo_restores_its_find() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 40, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    let find = repo::finds::list(
        &index,
        &repo::finds::FindFilter::default(),
        &PageRequest::default(),
    )
    .unwrap()
    .items
    .into_iter()
    .find(|f| f.status == FindStatus::Open && f.dig_ids.contains(&dig.id))
    .expect("the fixture mailbox raises an open Find");
    let message_id = find.sources[0].message_id.clone();
    let before = repo::classifications::current(&store, &message_id)
        .unwrap()
        .unwrap();

    let out = app
        .runtime
        .invoke_as_user(
            "decide_again",
            json!({ "digId": dig.id, "messageIds": [message_id] }),
        )
        .await
        .unwrap();
    assert_eq!(out.result["count"], json!(1), "{}", out.summary);
    assert_eq!(out.summary, "Sent 1 message back to Sift");
    let closed = repo::finds::get(&index, &find.id).unwrap().unwrap();
    assert_eq!(closed.status, FindStatus::Superseded);
    ops.runner.run_until_idle(Duration::from_secs(60)).await;
    let after = repo::classifications::current(&store, &message_id)
        .unwrap()
        .expect("Sift decided it again");
    assert_ne!(after.id, before.id, "a fresh decision");
    assert_ne!(after.mechanism, DecisionMechanism::User);

    let undone = app
        .runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": out.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    let report: UndoReport = serde_json::from_value(undone.result).unwrap();
    assert!(report.skipped.is_empty(), "{report:?}");
    let restored = repo::classifications::current(&store, &message_id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.disposition, before.disposition);
    assert_eq!(restored.reason, before.reason);
    assert_eq!(
        repo::finds::get(&index, &find.id).unwrap().unwrap().status,
        FindStatus::Open,
        "the Find is back"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_message_the_user_decided_is_left_alone() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 10, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let m = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter::default(),
        &PageRequest::default(),
    )
    .unwrap()
    .items[0]
        .id
        .clone();
    app.runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": m, "disposition": { "kind": "filed" } }),
        )
        .await
        .unwrap();
    let mine = repo::classifications::current(&store, &m).unwrap().unwrap();
    let out = app
        .runtime
        .invoke_as_user(
            "decide_again",
            json!({ "digId": dig.id, "messageIds": [m] }),
        )
        .await
        .unwrap();
    assert_eq!(out.result["count"], json!(0));
    assert_eq!(out.result["keptYours"], json!(1));
    assert_eq!(
        repo::classifications::current(&store, &m)
            .unwrap()
            .unwrap()
            .id,
        mine.id
    );
}
