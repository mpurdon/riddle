//! An account's labels are listed with their names and counts. Messages
//! carry the provider's label ids (`Label_4452336792128208639` on 2,200
//! messages of a real mailbox), so a sync now stores the account's label
//! list, and the Labels page and the Inbox filter read names from it.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sync_stores_the_label_list_and_labels_are_named_and_counted() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 20, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    // A user's own label, known by id on the message and by name in the list.
    repo::provider_labels::replace_all(
        &store,
        &[
            ProviderLabel {
                id: "INBOX".into(),
                name: "INBOX".into(),
                managed_by_riddle: false,
                system: true,
            },
            ProviderLabel {
                id: "Label_28".into(),
                name: "Receipts/2026".into(),
                managed_by_riddle: false,
                system: false,
            },
            ProviderLabel {
                id: "Label_99".into(),
                name: "Empty".into(),
                managed_by_riddle: false,
                system: false,
            },
        ],
    )
    .unwrap();
    let id = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        1,
    )
    .unwrap()
    .remove(0);
    repo::messages::set_labels(
        &store,
        &id,
        &["INBOX".into(), "Label_28".into(), "UNREAD".into()],
    )
    .unwrap();

    let labels = app.list_labels(Some(&dig.id)).unwrap();
    let find = |id: &str| labels.iter().find(|l| l.id == id).cloned();
    let receipts = find("Label_28").expect("the user's label is listed");
    assert_eq!(
        (
            receipts.name.as_str(),
            receipts.kind,
            receipts.messages,
            receipts.unread
        ),
        ("Receipts/2026", LabelKind::User, 1, 1)
    );
    assert_eq!(find("INBOX").unwrap().kind, LabelKind::System);
    assert!(
        find("Label_99").is_some(),
        "an empty label of the user's stays listed"
    );
    assert!(
        find("UNREAD").is_none(),
        "Unread is bookkeeping, not a label to browse"
    );

    // The next sync replaces the list with what the provider has.
    ops.queue
        .enqueue(riddle_jobs::JobSpec::new(JobKind::IncrementalSync, "Sync").dig(dig.id.clone()))
        .unwrap();
    ops.runner
        .run_until_idle(std::time::Duration::from_secs(60))
        .await;
    assert!(
        repo::provider_labels::all(&store)
            .unwrap()
            .iter()
            .all(|l| l.id != "Label_99"),
        "a sync refreshed the list from the provider"
    );
}
