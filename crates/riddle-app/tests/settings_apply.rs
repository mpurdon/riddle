//! Saved settings reach the running app without a restart, and keys stored
//! from Settings never leave the keychain. Boot read every setting once and
//! nothing listened afterwards, so autonomy, budgets, the Gmail client and
//! the judge kept their launch values until the next start.

use riddle_app::App;
use riddle_core::*;
use riddle_settings::{JudgeProvider, Settings, SettingsStore};
use riddle_store::repo;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

mod common;

async fn booted() -> (Arc<App>, Arc<SettingsStore>) {
    let (app, ops) = common::boot_scripted(false, |s| s.judge.provider = JudgeProvider::Null).await;
    (app, ops.settings.clone())
}

async fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..100 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{what} was not applied");
}

fn save(settings: &SettingsStore, f: impl FnOnce(&mut Settings)) -> Settings {
    let mut next = settings.settings();
    f(&mut next);
    settings.save(next).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_autonomy_level_governs_the_next_action_without_a_restart() {
    let (app, settings) = booted().await;
    app.runtime.arm(true);
    save(&settings, |s| s.autonomy.default = Autonomy::Act);
    eventually("autonomy", || {
        app.runtime.policy.read().default == Autonomy::Act
    })
    .await;
    assert!(
        app.runtime.is_armed(),
        "saving settings must not disarm a dev build"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gmail_client_id_saved_after_launch_reaches_the_provider_factory() {
    let (app, settings) = booted().await;
    let factory = app
        .services
        .get::<riddle_gmail::GmailProviderFactory>()
        .unwrap();
    assert!(factory.config().is_none());
    save(&settings, |s| {
        s.gmail.client_id = "123-abc.apps.googleusercontent.com".into()
    });
    eventually("gmail client id", || factory.config().is_some()).await;
    assert_eq!(
        factory.config().unwrap().client_id,
        "123-abc.apps.googleusercontent.com"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_the_judge_provider_in_settings_attaches_the_new_judge() {
    let (app, settings) = booted().await;
    assert!(app.services.judge().is_none());
    save(&settings, |s| s.judge.provider = JudgeProvider::Scripted);
    eventually("judge", || app.services.judge().is_some()).await;
    save(&settings, |s| s.judge.provider = JudgeProvider::Null);
    eventually("judge removal", || app.services.judge().is_none()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stored_key_is_present_and_never_recorded_in_activity() {
    let (app, settings) = booted().await;
    let value = "jev-test-key-do-not-record";
    let out = app
        .runtime
        .invoke_as_user(
            "set_secret",
            json!({ "secret": "jev_api_key", "value": value }),
        )
        .await
        .unwrap();
    let mut s = settings.settings();
    s.judge.key_source = riddle_settings::KeySource::Keychain;
    let s = settings.save(s).unwrap();
    assert!(s.judge.api_key_present);

    let activity = repo::activities::get(&app.stores.index(), &out.activity_id.unwrap())
        .unwrap()
        .unwrap();
    let recorded = serde_json::to_string(&activity).unwrap();
    assert!(!recorded.contains(value));
    assert!(recorded.contains("jev_api_key"));

    app.runtime
        .invoke_as_user("clear_secret", json!({ "secret": "jev_api_key" }))
        .await
        .unwrap();
    assert!(!settings.settings().judge.api_key_present);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_cannot_store_a_key_and_leaves_no_pending_copy_of_it() {
    let (app, _settings) = booted().await;
    let err = app
        .runtime
        .invoke(ActionInvocation {
            action: "set_secret".into(),
            args: json!({ "secret": "jev_api_key", "value": "from-an-agent" }),
            principal: Principal::Director {
                conversation_id: ConversationId::new(),
                read_untrusted: false,
            },
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind.as_str(), "policy_denied");
    assert!(repo::pending::list_pending(&app.stores.index())
        .unwrap()
        .is_empty());
    let creds = app
        .services
        .get::<riddle_credentials::Credentials>()
        .unwrap();
    assert!(!creds.present("jev_api_key"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_known_secrets_can_be_stored() {
    let (app, _settings) = booted().await;
    let err = app
        .runtime
        .invoke_as_user(
            "set_secret",
            json!({ "secret": "gmail_refresh_token:dig-1", "value": "x" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind.as_str(), "invalid");
}

const CLIENT_FILE: &str = r#"{"installed":{"client_id":"123-abc.apps.googleusercontent.com","project_id":"riddle-509505","client_secret":"GOCSPX-import-test","redirect_uris":["http://localhost"]}}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn importing_googles_client_file_sets_the_id_and_keeps_the_secret_in_the_keychain() {
    let (app, settings) = booted().await;
    let out = app
        .runtime
        .invoke_as_user("import_gmail_client", json!({ "text": CLIENT_FILE }))
        .await
        .unwrap();
    let s = settings.settings();
    assert_eq!(s.gmail.client_id, "123-abc.apps.googleusercontent.com");
    assert!(s.gmail.client_secret_present);
    assert!(!std::fs::read_to_string(settings.config_path())
        .unwrap_or_default()
        .contains("GOCSPX"));

    let activity = repo::activities::get(&app.stores.index(), &out.activity_id.unwrap())
        .unwrap()
        .unwrap();
    assert!(!serde_json::to_string(&activity).unwrap().contains("GOCSPX"));

    let factory = app
        .services
        .get::<riddle_gmail::GmailProviderFactory>()
        .unwrap();
    eventually("gmail client id", || factory.config().is_some()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_file_without_a_secret_removes_a_stale_one() {
    let (app, settings) = booted().await;
    app.runtime
        .invoke_as_user("import_gmail_client", json!({ "text": CLIENT_FILE }))
        .await
        .unwrap();
    app.runtime
        .invoke_as_user(
            "import_gmail_client",
            json!({ "text": r#"{"installed":{"client_id":"456-def.apps.googleusercontent.com"}}"# }),
        )
        .await
        .unwrap();
    let s = settings.settings();
    assert_eq!(s.gmail.client_id, "456-def.apps.googleusercontent.com");
    assert!(!s.gmail.client_secret_present);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_cannot_import_a_client_file() {
    let (app, settings) = booted().await;
    let err = app
        .runtime
        .invoke(ActionInvocation {
            action: "import_gmail_client".into(),
            args: json!({ "text": CLIENT_FILE }),
            principal: Principal::Mcp,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind.as_str(), "policy_denied");
    assert!(settings.settings().gmail.client_id.is_empty());
}
