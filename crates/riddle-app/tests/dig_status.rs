//! What a Dig's status says right after connecting. A freshly signed-in
//! Gmail Dig used to read "Not connected" until its first page of mail
//! arrived, and a first sync that crashed left it there with no reason;
//! a Degraded status, once saved, never cleared.

use riddle_app::ops::AppDigStatus;
use riddle_app::App;
use riddle_core::*;
use riddle_store::repo;
use std::sync::Arc;

mod common;

async fn booted() -> Arc<App> {
    common::boot_scripted(false, |_| {}).await.0
}

fn gmail_dig(app: &App) -> Dig {
    let dig = Dig {
        id: DigId::new(),
        name: "Gmail".into(),
        email: "me@example.com".into(),
        provider: ProviderKind::Gmail,
        group: None,
        privacy: PrivacyMode::CloudJudgment,
        projection: ProjectionMode::Full,
        status: DigStatus::NotConnected,
        shadow_started_at: None,
        created_at: chrono::Utc::now(),
        disabled: false,
        history_days: None,
        mailbox: None,
    };
    repo::digs::upsert(&app.stores.index(), &dig).unwrap();
    dig
}

fn sign_in(app: &App, dig: &Dig) {
    app.services
        .get::<riddle_credentials::Credentials>()
        .unwrap()
        .set(
            &riddle_credentials::gmail_refresh_token_key(&dig.id),
            "token",
        )
        .unwrap();
}

fn sync_job(app: &App, dig: &Dig, kind: JobKind, status: JobStatus) {
    let now = chrono::Utc::now();
    let job = Job {
        id: JobId::new(),
        kind,
        priority: Priority::CatchUp,
        status,
        dig_id: Some(dig.id.clone()),
        label: "sync".into(),
        progress: Progress::default(),
        checkpoint: serde_json::Value::Null,
        cancellable: true,
        attempts: 1,
        created_at: now,
        started_at: Some(now),
        finished_at: Some(now),
        updated_at: now,
    };
    repo::jobs::upsert(&app.stores.index(), &job).unwrap();
}

fn status(app: &Arc<App>, dig: &Dig) -> DigStatus {
    app.refresh_dig_status(&dig.id);
    let dig = app.dig(&dig.id).unwrap();
    AppDigStatus(app.clone()).status_of(&dig).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_signed_in_gmail_dig_with_no_mail_yet_is_catching_up_not_disconnected() {
    let app = booted().await;
    let dig = gmail_dig(&app);
    assert_eq!(status(&app, &dig), DigStatus::NotConnected);
    sign_in(&app, &dig);
    assert!(matches!(
        status(&app, &dig),
        DigStatus::CatchingUp { total: 0, .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_first_sync_shows_its_reason_and_clears_when_a_later_sync_runs() {
    let app = booted().await;
    let dig = gmail_dig(&app);
    sign_in(&app, &dig);
    sync_job(
        &app,
        &dig,
        JobKind::InitialSync,
        JobStatus::Failed {
            error: "job handler panicked".into(),
        },
    );
    match status(&app, &dig) {
        DigStatus::Degraded { reason } => assert!(reason.contains("first sync failed")),
        other => panic!("expected Degraded, got {other:?}"),
    }
    std::thread::sleep(std::time::Duration::from_millis(5));
    sync_job(&app, &dig, JobKind::IncrementalSync, JobStatus::Running);
    assert!(matches!(status(&app, &dig), DigStatus::CatchingUp { .. }));
}
