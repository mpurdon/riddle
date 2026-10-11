//! An account that is not syncing says so. Every sync of a real Gmail
//! account failed on an expired sign-in for two weeks, twenty thousand
//! times, while the job reported "completed" and the account "Caught up".
//! Now any failure is recorded: a refused sign-in shows at once and
//! interrupts once, a network outage only once it has lasted, and the next
//! sync that gets through clears it.

use riddle_core::*;
use riddle_errors::ErrorKind;
use riddle_store::repo;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failing_sync_shows_on_the_account_and_a_sync_that_gets_through_clears_it() {
    // No background syncs: the fixture's would succeed and clear the
    // failures this test records.
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    let mut events = app.events.subscribe();
    let mut notes = || {
        let mut n = Vec::new();
        while let Ok(e) = events.try_recv() {
            if let RiddleEvent::Notify { title, .. } = e {
                n.push(title);
            }
        }
        n
    };
    let status = || app.dig(&dig.id).unwrap().status;

    // A refused sign-in shows at once, and interrupts once however often it fails.
    app.record_sync_result(&dig.id, Some(ErrorKind::ProviderAuth));
    app.record_sync_result(&dig.id, Some(ErrorKind::ProviderAuth));
    assert!(matches!(
        status(),
        DigStatus::SyncFailing {
            failure: SyncFailure::SignIn,
            ..
        }
    ));
    let told = notes();
    assert_eq!(told, vec!["Personal is not syncing".to_string()]);

    // Sync now says why it cannot, instead of "Syncing now".
    let err = app
        .runtime
        .invoke_as_user("refresh_now", json!({ "digId": dig.id }))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::ProviderAuth, "{}", err.message);

    app.record_sync_result(&dig.id, None);
    assert!(!matches!(status(), DigStatus::SyncFailing { .. }));
    app.runtime
        .invoke_as_user("refresh_now", json!({ "digId": dig.id }))
        .await
        .unwrap();

    // A dropped connection is quiet at first...
    app.record_sync_result(&dig.id, Some(ErrorKind::Provider));
    assert!(!matches!(status(), DigStatus::SyncFailing { .. }));
    // ...and shows once it has lasted a quarter of an hour.
    let store = app.stores.dig(&dig.id).unwrap();
    let earlier = (chrono::Utc::now() - chrono::Duration::minutes(20)).to_rfc3339();
    repo::sync_state::set(&store, repo::sync_state::SYNC_FAILING_SINCE, &earlier).unwrap();
    app.record_sync_result(&dig.id, Some(ErrorKind::Provider));
    match status() {
        DigStatus::SyncFailing {
            failure,
            last_synced_at,
            ..
        } => {
            assert_eq!(failure, SyncFailure::Unreachable);
            assert!(last_synced_at.is_some(), "the fixture account synced once");
        }
        other => panic!("expected SyncFailing, got {other:?}"),
    }
}

/// A refused Gmail sign-in with no OAuth client configured is a missing
/// setup, not an expired sign-in: "Sign in again" could only fail the same
/// way. Saving a client clears it so Sync now runs at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_gmail_client_reads_as_not_set_up_until_one_is_saved() {
    let (app, ops) = common::boot_scripted(false, |s| s.gmail.client_id = String::new()).await;
    let fixture =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    // The same account as a Gmail one, without its sign-in ever working here.
    let gmail = Dig {
        provider: ProviderKind::Gmail,
        ..app.dig(&fixture.id).unwrap()
    };
    repo::digs::upsert(&app.stores.index(), &gmail).unwrap();

    app.record_sync_result(&gmail.id, Some(ErrorKind::ProviderAuth));
    match app.dig(&gmail.id).unwrap().status {
        DigStatus::SyncFailing { failure, .. } => assert_eq!(failure, SyncFailure::NotSetUp),
        other => panic!("expected SyncFailing, got {other:?}"),
    }
    let err = app
        .runtime
        .invoke_as_user("refresh_now", json!({ "digId": gmail.id }))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("client id is missing"),
        "{}",
        err.message
    );

    let mut settings = app
        .services
        .require::<riddle_settings::SettingsStore>()
        .unwrap()
        .settings();
    settings.gmail.client_id = "1234-abc.apps.googleusercontent.com".into();
    app.runtime
        .invoke(ActionInvocation {
            action: "save_settings".into(),
            args: json!({ "settings": settings }),
            principal: Principal::User,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
        .unwrap();
    // The settings watcher applies the change; wait for it to clear.
    for _ in 0..50 {
        if !matches!(
            app.dig(&gmail.id).unwrap().status,
            DigStatus::SyncFailing { .. }
        ) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("saving a client did not clear \"not set up\"");
}
