//! Sift changes the mailbox only at Act and above, and a dev build on Hold
//! or Ask sends nothing by itself. Projection reached Gmail outside the
//! Action layer, so a Dig at Observe with "Full" projection would have
//! labelled and archived the user's real mail as soon as Sift ran. Your own
//! corrections always count in Riddle: on Hold their mailbox change waits,
//! and on Ask a batch of them goes with one confirmation. A batch of seven
//! corrections used to be refused seven times by an unarmed build.

use riddle_app::App;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::sync::Arc;

mod common;

async fn sifted_fixture(autonomy: Autonomy) -> (Arc<App>, Dig) {
    let (app, ops) = common::boot_scripted(true, |s| s.autonomy.default = autonomy).await;
    let dig = common::fixture_dig_synced(
        &app,
        &ops,
        "Work",
        "me@example.com",
        5,
        60,
        json!({ "projection": "full" }),
    )
    .await;
    let dig = app.dig(&dig.id).unwrap();
    let store = app.stores.dig(&dig.id).unwrap();
    assert!(
        repo::messages::count(&store).unwrap() > 0,
        "the fixture synced"
    );
    assert_eq!(
        repo::messages::unclassified_count(&store).unwrap(),
        0,
        "Sift ran"
    );
    (app, dig)
}

fn pending(app: &App, dig: &Dig) -> u64 {
    repo::outbox::pending_count(&app.stores.dig(&dig.id).unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_observe_sift_classifies_but_queues_nothing_for_the_mailbox() {
    let (app, dig) = sifted_fixture(Autonomy::Observe).await;
    assert_eq!(pending(&app, &dig), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_suggest_sift_still_queues_nothing_for_the_mailbox() {
    let (app, dig) = sifted_fixture(Autonomy::Suggest).await;
    assert_eq!(pending(&app, &dig), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_act_changes_wait_until_a_dev_build_is_armed() {
    let (app, dig) = sifted_fixture(Autonomy::Act).await;
    let queued = pending(&app, &dig);
    assert!(queued > 0, "Act should queue Sift's changes");

    app.projection_pass(&dig).await.unwrap();
    assert_eq!(pending(&app, &dig), queued, "unarmed: nothing sent");

    app.runtime.arm(true);
    app.projection_pass(&dig).await.unwrap();
    assert_eq!(pending(&app, &dig), 0, "armed: the queue drains");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_observe_your_own_move_to_spoil_is_still_queued_for_the_mailbox() {
    let (app, dig) = sifted_fixture(Autonomy::Observe).await;
    assert_eq!(pending(&app, &dig), 0);
    let store = app.stores.dig(&dig.id).unwrap();
    let id = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        1,
    )
    .unwrap()
    .remove(0);
    // On Hold: the move is projected, so it runs and its write waits.
    app.runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": [id] }),
        )
        .await
        .unwrap();
    assert!(
        pending(&app, &dig) > 0,
        "a user move reaches the outbox at Observe"
    );
}

fn first_ids(app: &App, dig: &Dig, n: usize) -> Vec<MessageId> {
    let store = app.stores.dig(&dig.id).unwrap();
    let mut ids = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        100,
    )
    .unwrap();
    ids.truncate(n);
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_hold_corrections_count_in_riddle_and_their_mailbox_changes_wait() {
    let (app, dig) = sifted_fixture(Autonomy::Observe).await;
    assert_eq!(app.runtime.writes(), ProviderWrites::Hold);
    for id in first_ids(&app, &dig, 3) {
        app.runtime
            .invoke_as_user(
                "set_disposition",
                json!({ "digId": dig.id, "messageId": id, "disposition": { "kind": "filed" } }),
            )
            .await
            .unwrap();
        let k = repo::classifications::current(&app.stores.dig(&dig.id).unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(k.disposition, Disposition::Filed);
    }
    let queued = pending(&app, &dig);
    assert!(queued > 0);
    assert_eq!(app.mailbox_writes().unwrap().held, queued);
    app.projection_pass(&dig).await.unwrap();
    assert_eq!(pending(&app, &dig), queued, "Hold sends nothing");
    // Sending them yourself is refused on Hold too.
    let err = app
        .runtime
        .invoke(ActionInvocation {
            confirmed: true,
            ..user("send_held_changes")
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind, riddle_errors::ErrorKind::PolicyDenied);
}

fn user(action: &str) -> ActionInvocation {
    ActionInvocation {
        action: action.into(),
        args: json!({}),
        principal: Principal::User,
        dig_id: None,
        idempotency_key: None,
        confirmed: false,
        pending_id: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_ask_waiting_changes_go_in_one_confirmed_batch_or_are_dropped() {
    let (app, dig) = sifted_fixture(Autonomy::Observe).await;
    app.runtime.set_writes(ProviderWrites::Ask);
    let ids = first_ids(&app, &dig, 4);
    app.runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": ids[..3] }),
        )
        .await
        .unwrap();
    assert!(pending(&app, &dig) > 0);
    app.projection_pass(&dig).await.unwrap();
    assert!(pending(&app, &dig) > 0, "Ask waits for the user");

    let err = app
        .runtime
        .invoke(user("send_held_changes"))
        .await
        .unwrap_err();
    assert_eq!(err.kind, riddle_errors::ErrorKind::NeedsConfirmation);
    let asked: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
    app.runtime
        .invoke(ActionInvocation {
            pending_id: Some(asked.id),
            ..user("send_held_changes")
        })
        .await
        .unwrap();
    assert_eq!(pending(&app, &dig), 0, "one confirmation sent the batch");

    app.runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": [ids[3].clone()] }),
        )
        .await
        .unwrap();
    assert!(pending(&app, &dig) > 0);
    app.runtime
        .invoke(ActionInvocation {
            confirmed: true,
            ..user("discard_held_changes")
        })
        .await
        .unwrap();
    assert_eq!(pending(&app, &dig), 0);
    let k = repo::classifications::current(&app.stores.dig(&dig.id).unwrap(), &ids[3])
        .unwrap()
        .unwrap();
    assert_eq!(
        k.disposition,
        Disposition::Spoil,
        "Riddle keeps its decision"
    );
}
