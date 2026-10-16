//! Adding a sender to contacts decides all their mail again as a contact's;
//! mail the user decided stays as it is, and undo takes the contact out and
//! puts the earlier decisions back (15 Oct 2026).

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::time::Duration;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_contact_s_mail_is_decided_again_and_undo_puts_it_back() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 40, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    // The sender with the most mail in the fixture mailbox.
    let ids = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        10_000,
    )
    .unwrap();
    let mut by_sender: std::collections::BTreeMap<String, Vec<MessageId>> = Default::default();
    for id in &ids {
        let m = repo::messages::get(&store, &dig.id, id).unwrap().unwrap();
        if !m.is_sent {
            by_sender
                .entry(m.from.email.to_ascii_lowercase())
                .or_default()
                .push(id.clone());
        }
    }
    let (email, theirs) = by_sender.into_iter().max_by_key(|(_, v)| v.len()).unwrap();
    assert!(theirs.len() >= 2, "{email}: {}", theirs.len());
    // One of theirs the user decided themselves.
    let yours = theirs[0].clone();
    app.runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": yours, "disposition": { "kind": "filed" } }),
        )
        .await
        .unwrap();
    ops.runner.run_until_idle(Duration::from_secs(60)).await;
    let before: Vec<_> = theirs
        .iter()
        .map(|id| repo::classifications::current(&store, id).unwrap().unwrap())
        .collect();

    let out = app
        .runtime
        .invoke_as_user("add_to_contacts", json!({ "email": email }))
        .await
        .unwrap();
    assert_eq!(
        out.result["count"],
        json!(theirs.len() - 1),
        "{}",
        out.summary
    );
    assert_eq!(out.result["keptYours"], json!(1));
    assert!(repo::user_profile::get(&index).unwrap().is_contact(&email));
    ops.runner.run_until_idle(Duration::from_secs(60)).await;
    for (id, b) in theirs.iter().zip(&before) {
        let now = repo::classifications::current(&store, id).unwrap().unwrap();
        if *id == yours {
            assert_eq!(now.id, b.id, "your decision stands");
        } else {
            assert_ne!(now.id, b.id, "decided again");
        }
    }
    let m = repo::messages::get(&store, &dig.id, &theirs[1])
        .unwrap()
        .unwrap();
    let facts = app
        .sift_input(&dig, m)
        .map(|i| i.people.in_contacts)
        .unwrap();
    assert!(facts, "Sift reads them as a contact");

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
    assert!(!repo::user_profile::get(&index).unwrap().is_contact(&email));
    for (id, b) in theirs.iter().zip(&before) {
        let now = repo::classifications::current(&store, id).unwrap().unwrap();
        assert_eq!(
            (now.disposition.clone(), now.reason.clone()),
            (b.disposition.clone(), b.reason.clone())
        );
    }
}
