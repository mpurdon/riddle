//! An account can be switched off without removing it. The fixture Work
//! account sat beside the user's real Gmail: its Finds in the shared list,
//! its mail in every preview of a rule. Removing it purges the store; a
//! disabled account keeps its mail and decisions, is neither synced nor
//! sifted, and drops out of everything that spans accounts until it is
//! enabled again.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::time::Duration;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_disabled_account_keeps_its_mail_but_leaves_every_list_until_it_is_enabled() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let work =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 3, 60, json!({})).await;
    let personal =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 4, 60, json!({})).await;
    let index = app.stores.index();
    let work_store = app.stores.dig(&work.id).unwrap();
    let messages_before = repo::messages::count(&work_store).unwrap();
    let work_finds = |app: &riddle_app::App| {
        app.list_finds(&repo::finds::FindFilter::default(), &PageRequest::default())
            .unwrap()
            .items
            .into_iter()
            .filter(|f| f.sources.iter().all(|s| s.dig_id == work.id))
            .count()
    };
    assert!(work_finds(&app) > 0, "the fixture produces Finds on Work");

    app.runtime
        .invoke_as_user(
            "set_dig_enabled",
            json!({ "digId": work.id, "enabled": false }),
        )
        .await
        .unwrap();

    let ids = |digs: Vec<Dig>| digs.into_iter().map(|d| d.id).collect::<Vec<_>>();
    assert_eq!(ids(app.list_digs().unwrap()), vec![personal.id.clone()]);
    assert_eq!(
        ids(app.list_all_digs().unwrap()).len(),
        2,
        "account management still sees it"
    );
    assert_eq!(work_finds(&app), 0, "its Finds leave the list");
    let open_personal = repo::finds::count_open(&index, Some(&personal.id)).unwrap();
    assert_eq!(
        repo::finds::count_open(&index, None).unwrap(),
        open_personal,
        "and the count"
    );
    assert_eq!(
        repo::messages::count(&work_store).unwrap(),
        messages_before,
        "its mail is kept"
    );

    // New mail at the provider is not synced while it is off.
    app.runtime
        .invoke_as_user(
            "inject_fixture_message",
            json!({ "digId": work.id, "subject": "While disabled" }),
        )
        .await
        .unwrap();
    let sync_work = || {
        ops.queue
            .enqueue(
                riddle_jobs::JobSpec::new(JobKind::IncrementalSync, "Sync Work")
                    .dig(work.id.clone()),
            )
            .unwrap();
    };
    sync_work();
    ops.runner.run_until_idle(Duration::from_secs(60)).await;
    assert_eq!(repo::messages::count(&work_store).unwrap(), messages_before);

    // Saving its other settings from a copy does not switch it back on.
    let mut edited = repo::digs::get(&index, &work.id).unwrap().unwrap();
    edited.name = "Work (test)".into();
    edited.disabled = false;
    app.runtime
        .invoke_as_user("update_dig", json!({ "dig": edited }))
        .await
        .unwrap();
    assert!(repo::digs::get(&index, &work.id).unwrap().unwrap().disabled);

    // Enabled again: it catches up and its Finds come back.
    app.runtime
        .invoke_as_user(
            "set_dig_enabled",
            json!({ "digId": work.id, "enabled": true }),
        )
        .await
        .unwrap();
    sync_work();
    ops.runner.run_until_idle(Duration::from_secs(60)).await;
    assert_eq!(
        repo::messages::count(&work_store).unwrap(),
        messages_before + 1
    );
    assert!(work_finds(&app) > 0);
}
