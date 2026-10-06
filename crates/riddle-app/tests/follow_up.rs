//! Follow up by a date: `set_disposition` with `dueAt` opens a Follow Up
//! Find due then; a later date on the same message moves that Find rather
//! than opening a second one, and undo puts the earlier date back; a date
//! in the past is refused before anything is written. Until this the only
//! way to a Follow Up Find from a message was the Correct… menu, with no
//! date at all.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

/// A synced fixture account and one of its messages that no Find names,
/// by thread or by source, so a follow-up opens a Find rather than joining
/// Sift's. (The GitHub default Mesh now raises Review Finds from the
/// fixture's review requests without a judge, so the first message is no
/// longer a safe pick.)
async fn dig_and_message(app: &riddle_app::App, ops: &riddle_app::ops::Ops) -> (Dig, MessageId) {
    let dig =
        common::fixture_dig_synced(app, ops, "Work", "me@example.com", 5, 20, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    let messages = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter::default(),
        &PageRequest::default(),
    )
    .unwrap();
    let m = messages
        .items
        .iter()
        .find(|m| {
            // A thread starter, so the Find's identity key is its own
            // Message-ID and cannot join a Find raised on a reply elsewhere.
            let full = repo::messages::get(&store, &dig.id, &m.id)
                .unwrap()
                .unwrap();
            let starter = full.headers.references.is_empty() && full.headers.in_reply_to.is_none();
            let key = full
                .headers
                .message_id
                .clone()
                .unwrap_or_else(|| m.thread_id.provider_id().to_string());
            starter
                && repo::finds::open_by_thread(&index, m.thread_id.provider_id())
                    .unwrap()
                    .is_empty()
                && repo::finds::for_message(&index, &m.id).unwrap().is_empty()
                && repo::finds::any_by_identity(&index, &key)
                    .unwrap()
                    .is_empty()
        })
        .expect("a message whose thread has no open Find")
        .id
        .clone();
    (dig, m)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn marking_a_message_for_follow_up_opens_a_find_due_then_and_a_later_date_moves_it() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let (dig, m) = dig_and_message(&app, &ops).await;
    let index = app.stores.index();
    let now = chrono::Utc::now();
    let first = now + chrono::Duration::days(1);

    let out = app
        .runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": m, "disposition": "find", "findType": "follow_up", "dueAt": first }),
        )
        .await
        .unwrap();
    assert!(
        out.summary.starts_with("Marked for follow-up by "),
        "the Activity names the date: {}",
        out.summary
    );
    let id: FindId = serde_json::from_value(out.result["findId"].clone()).unwrap();
    let f = repo::finds::get(&index, &id).unwrap().unwrap();
    assert_eq!(f.find_type, FindType::FollowUp);
    assert_eq!(f.status, FindStatus::Open);
    assert_eq!(f.due_at, Some(first));
    let store = app.stores.dig(&dig.id).unwrap();
    assert_eq!(
        repo::classifications::current(&store, &m)
            .unwrap()
            .unwrap()
            .disposition,
        Disposition::Find {
            find_type: FindType::FollowUp
        }
    );

    // The same message again, later: the one Find moves.
    let later = now + chrono::Duration::days(3);
    let again = app
        .runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": m, "disposition": "find", "findType": "follow_up", "dueAt": later }),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_value::<FindId>(again.result["findId"].clone()).unwrap(),
        id,
        "a second date joins the open Find rather than opening another"
    );
    let f = repo::finds::get(&index, &id).unwrap().unwrap();
    assert_eq!(f.due_at, Some(later));

    // Undo of the second decision puts the first date back.
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": again.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    let f = repo::finds::get(&index, &id).unwrap().unwrap();
    assert_eq!(f.due_at, Some(first));
    assert_eq!(f.status, FindStatus::Open);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_follow_up_date_in_the_past_is_refused_and_nothing_is_written() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let (dig, m) = dig_and_message(&app, &ops).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let before = repo::classifications::current(&store, &m).unwrap();
    let past = chrono::Utc::now() - chrono::Duration::hours(1);
    let err = app
        .runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": m, "disposition": "find", "findType": "follow_up", "dueAt": past }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid, "{err:?}");
    let after = repo::classifications::current(&store, &m).unwrap();
    assert_eq!(
        after.map(|k| k.id),
        before.map(|k| k.id),
        "the classification is untouched"
    );
    assert!(
        repo::finds::for_message(&app.stores.index(), &m)
            .unwrap()
            .is_empty(),
        "no Find was opened"
    );
    // A date on anything but a Find is a mistake, not a silent drop.
    let err = app
        .runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": m, "disposition": "filed", "dueAt": chrono::Utc::now() + chrono::Duration::days(1) }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid, "{err:?}");
}
