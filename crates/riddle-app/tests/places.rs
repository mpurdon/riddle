//! The mail column (option D of the mail-navigation study): a Stratum and
//! the person's label of the same name are one Topic, counted once per
//! message; the other labels stay labels, empty ones included; a pin moves
//! a place to the top; and the Inbox lists a place's mail in each account
//! by that account's own label ids.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_label_and_a_stratum_of_one_name_are_one_topic_and_pins_move_places_to_the_top() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 8, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let ids = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        10_000,
    )
    .unwrap();
    assert!(ids.len() >= 5, "the fixture account has mail");
    let label = |id: &str, name: &str| ProviderLabel {
        id: id.into(),
        name: name.into(),
        managed_by_riddle: false,
        system: false,
    };
    repo::provider_labels::replace_all(
        &store,
        &[
            label("Label_1", "Receipts"),
            label("Label_2", "Work/Receipts"),
            label("Label_3", "travel"),
            label("Label_4", "Unwanted"),
        ],
    )
    .unwrap();
    // Start from no labels and no Strata, then: both, the Stratum only,
    // the label only, and a nested label of the same leaf name.
    for id in &ids {
        repo::messages::set_labels(&store, id, &[]).unwrap();
        repo::messages::set_strata(&store, id, &[]).unwrap();
    }
    repo::messages::set_labels(&store, &ids[0], &["Label_1".into(), "UNREAD".into()]).unwrap();
    repo::messages::set_strata(&store, &ids[0], &["Receipts".into()]).unwrap();
    repo::messages::set_strata(&store, &ids[1], &["Receipts".into()]).unwrap();
    repo::messages::set_labels(&store, &ids[2], &["Label_1".into()]).unwrap();
    repo::messages::set_labels(&store, &ids[3], &["Label_2".into()]).unwrap();

    let places = app.mail_places(None).unwrap();
    let receipts = places
        .topics
        .iter()
        .find(|t| t.name == "Receipts")
        .expect("a Receipts Topic");
    assert_eq!(
        (receipts.messages, receipts.unread, receipts.has_label),
        (3, 1, true),
        "three messages, the one with both counted once"
    );
    // A label merges without regard to case, even with no mail yet.
    let travel = places
        .topics
        .iter()
        .find(|t| t.name == "Travel")
        .expect("travel merges into Travel");
    assert!(travel.has_label);
    let names: Vec<&str> = places.labels.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        ["Unwanted", "Work/Receipts"],
        "the merged labels are Topics now; an empty label stays"
    );
    assert_eq!(places.mailboxes[0].place, Place::All);

    let ids_at = |place: &str| -> Vec<MessageId> {
        let mut v: Vec<MessageId> = app
            .list_messages_at(
                Some(&dig.id),
                &Default::default(),
                Some(&place.parse().unwrap()),
                &PageRequest::default(),
            )
            .unwrap()
            .items
            .into_iter()
            .map(|m| m.id)
            .collect();
        v.sort();
        v
    };
    let mut want = ids[..3].to_vec();
    want.sort();
    assert_eq!(ids_at("topic:Receipts"), want, "the label or the Stratum");
    assert_eq!(ids_at("label:Work/Receipts"), vec![ids[3].clone()]);
    assert!(
        ids_at("label:No such label").is_empty(),
        "a label this account lacks matches nothing"
    );

    // Pinning moves the place to Pinned; unpinning puts it back.
    app.runtime
        .invoke_as_user("pin_place", json!({ "place": "label:Work/Receipts" }))
        .await
        .unwrap();
    app.runtime
        .invoke_as_user("pin_place", json!({ "place": "topic:Receipts" }))
        .await
        .unwrap();
    let places = app.mail_places(None).unwrap();
    let pinned: Vec<String> = places.pinned.iter().map(|p| p.place.to_string()).collect();
    assert_eq!(
        pinned,
        ["label:Work/Receipts", "topic:Receipts"],
        "in the order pinned"
    );
    assert!(places.pinned.iter().all(|p| p.pinned));
    assert!(!places.labels.iter().any(|l| l.name == "Work/Receipts"));
    assert!(!places.topics.iter().any(|t| t.name == "Receipts"));
    app.runtime
        .invoke_as_user("unpin_place", json!({ "place": "label:Work/Receipts" }))
        .await
        .unwrap();
    let places = app.mail_places(None).unwrap();
    assert_eq!(places.pinned.len(), 1);
    assert!(places.labels.iter().any(|l| l.name == "Work/Receipts"));

    assert!(
        app.runtime
            .invoke_as_user("pin_place", json!({ "place": "folder:x" }))
            .await
            .is_err(),
        "not a place"
    );
}
