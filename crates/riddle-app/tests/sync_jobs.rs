//! The first sync as the job runner runs it (§53, §55): bounded listing
//! runs that yield so the queue sifts and fetches bodies for what has
//! landed, a restart that resumes from the token, and the history backfill
//! that follows the window. A large mailbox used to be listed whole inside
//! one job while the runner refused the Dig any other provider work, so
//! nothing was sorted until the listing finished (17 Oct 2026).

use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_jobs::JobSpec;
use riddle_settings::{JudgeProvider, LlmProvider, Settings};
use riddle_store::repo;
use riddle_sync::SyncOptions;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

mod common;

/// One page of a hundred a run, so a 300-message fixture takes three.
fn short_runs(app: &App) {
    app.services.insert(Arc::new(SyncOptions {
        pages_per_run: 1,
        ..Default::default()
    }));
}

async fn fixture_dig(app: &App, count: u32) -> Dig {
    let out = app
        .runtime
        .invoke_as_user(
            "add_fixture_dig",
            json!({ "name": "Work", "email": "matt@example.com", "seed": 11, "count": count }),
        )
        .await
        .unwrap();
    serde_json::from_value(out.result).unwrap()
}

fn listed_count(app: &App, dig: &Dig) -> u64 {
    let domains = app.services.get::<riddle_app::domains::Domains>().unwrap();
    domains
        .providers
        .fixture(&dig.id)
        .expect("the fixture provider exists once something synced")
        .snapshot()
        .iter()
        .filter(|m| !m.labels.iter().any(|l| l == "SPAM"))
        .count() as u64
}

fn list_calls(app: &App, dig: &Dig) -> u64 {
    let domains = app.services.get::<riddle_app::domains::Domains>().unwrap();
    domains
        .providers
        .fixture(&dig.id)
        .map(|f| f.call_count("list"))
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_sync_yields_between_listing_runs_and_a_sift_runs_between_them() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    short_runs(&app);
    let dig = fixture_dig(&app, 300).await;
    let id = ops
        .queue
        .enqueue(JobSpec::new(JobKind::InitialSync, "First sync of Work").dig(dig.id.clone()))
        .unwrap();
    assert!(ops.runner.run_until_idle(Duration::from_secs(180)).await);
    let store = app.stores.dig(&dig.id).unwrap();
    let expected = listed_count(&app, &dig);
    assert_eq!(repo::messages::count(&store).unwrap(), expected);
    assert_eq!(repo::messages::unclassified_count(&store).unwrap(), 0);
    assert_eq!(repo::messages::count_without_body(&store).unwrap(), 0);
    assert_eq!(
        list_calls(&app, &dig),
        4,
        "three runs of one page, no page twice; then the backfill's one empty page"
    );
    let recent = ops.queue.recent(100).unwrap();
    let initial = recent.iter().find(|j| j.id == id).unwrap();
    assert_eq!(initial.status, JobStatus::Completed);
    let sift = recent
        .iter()
        .filter(|j| j.kind == JobKind::Sift && j.dig_id.as_ref() == Some(&dig.id))
        .min_by_key(|j| j.created_at)
        .expect("the listing queued a Sift of what had landed");
    assert!(
        sift.started_at.unwrap() < initial.finished_at.unwrap(),
        "the Sift ran while the listing still had pages"
    );
    // The window listed, the history behind it followed and found nothing
    // older (the fixture spans sixty days), and the account is caught up.
    let backfill = recent
        .iter()
        .find(|j| j.kind == JobKind::HistoryBackfill && j.dig_id.as_ref() == Some(&dig.id))
        .expect("the backfill is queued when the window is listed");
    assert_eq!(backfill.status, JobStatus::Completed);
    assert!(!app.backfill_pending(&dig).unwrap());
    app.refresh_dig_status(&dig.id);
    assert_eq!(app.dig(&dig.id).unwrap().status, DigStatus::Synced);
    assert_eq!(app.task_centre().unwrap().overall, OverallStatus::CaughtUp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_restart_resumes_the_first_sync_from_its_token_rather_than_restarting() {
    let dir = std::env::temp_dir().join(format!("riddle-sync-jobs-{}", JobId::new()));
    let config = AppConfig {
        data_dir: Some(dir.join("data")),
        config_dir: Some(dir.join("config")),
        dev: true,
        build: BuildStamp::from_env_vars("0", None, None, None, "test"),
        keychain_service: "dev.codenaked.riddle.test".into(),
        // No keychain and no network: credentials stay in memory.
        offline: true,
    };
    let settings = || {
        let mut s = Settings::dev_defaults();
        s.judge.provider = JudgeProvider::Scripted;
        s.llm.provider = LlmProvider::Null;
        s
    };
    let app = App::open(config.clone()).unwrap();
    let ops = app.boot_with(false, Some(settings())).await.unwrap();
    short_runs(&app);
    let dig = fixture_dig(&app, 300).await;
    let id = ops
        .queue
        .enqueue(JobSpec::new(JobKind::InitialSync, "First sync of Work").dig(dig.id.clone()))
        .unwrap();
    // One run: a tick starts the job, which lists a page and yields.
    assert_eq!(ops.runner.tick(), 1);
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let job = ops.queue.get(&id).unwrap().unwrap();
            if job.status == JobStatus::Queued && job.progress.done >= 100 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the first run yields with its progress");
    let store = app.stores.dig(&dig.id).unwrap();
    assert_eq!(
        repo::sync_state::get(&store, riddle_sync::engine::INITIAL_LISTED)
            .unwrap()
            .as_deref(),
        Some("100")
    );
    assert!(!app.initial_sync_done(&dig).unwrap());
    drop(store);
    drop(ops);
    drop(app);

    // The app comes back: the job is queued again from the store, and the
    // listing goes on from page two.
    let app = App::open(config).unwrap();
    let ops = app.boot_with(false, Some(settings())).await.unwrap();
    short_runs(&app);
    let job = ops.queue.get(&id).unwrap().expect("the job survived");
    assert_eq!(job.status, JobStatus::Queued);
    assert!(ops.runner.run_until_idle(Duration::from_secs(180)).await);
    let store = app.stores.dig(&dig.id).unwrap();
    let expected = listed_count(&app, &dig);
    assert_eq!(repo::messages::count(&store).unwrap(), expected);
    assert_eq!(
        repo::sync_state::get(&store, riddle_sync::engine::INITIAL_LISTED)
            .unwrap()
            .as_deref(),
        Some(expected.to_string().as_str()),
        "listed on from a hundred, not from nought"
    );
    assert_eq!(
        list_calls(&app, &dig),
        3,
        "pages two and three only, then the backfill's one empty page"
    );
    assert!(app.initial_sync_done(&dig).unwrap());
    assert_eq!(repo::messages::unclassified_count(&store).unwrap(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_poll_during_the_first_listing_queues_the_first_sync_rather_than_listing_itself() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    short_runs(&app);
    let dig = fixture_dig(&app, 300).await;
    // The 45-second poll, or Sync now, before any first sync was queued.
    let poll = ops
        .queue
        .enqueue(JobSpec::new(JobKind::IncrementalSync, "Sync Work").dig(dig.id.clone()))
        .unwrap();
    assert!(ops.runner.run_until_idle(Duration::from_secs(180)).await);
    let recent = ops.queue.recent(100).unwrap();
    let poll = recent.iter().find(|j| j.id == poll).unwrap();
    assert_eq!(poll.status, JobStatus::Completed);
    assert_eq!(
        poll.progress.done, 0,
        "the poll listed nothing itself; the first sync job did"
    );
    let first = recent
        .iter()
        .find(|j| j.kind == JobKind::InitialSync && j.dig_id.as_ref() == Some(&dig.id))
        .expect("the poll queued the first sync");
    assert_eq!(first.status, JobStatus::Completed);
    let store = app.stores.dig(&dig.id).unwrap();
    assert_eq!(
        repo::messages::count(&store).unwrap(),
        listed_count(&app, &dig)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refresh_now_runs_a_sync_job_that_lands_an_injected_message_with_its_body() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "matt@example.com", 11, 40, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let before = repo::messages::count(&store).unwrap();
    let domains = app.services.get::<riddle_app::domains::Domains>().unwrap();
    let injected = domains
        .providers
        .fixture(&dig.id)
        .unwrap()
        .inject("custom", None, Some("Dinner Sunday?".into()), None)
        .unwrap();
    let out = app
        .runtime
        .invoke_as_user("refresh_now", json!({ "digId": dig.id }))
        .await
        .unwrap();
    assert_eq!(out.result["heard"], true);
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let landed = repo::messages::get(&store, &dig.id, &injected.id)
                .unwrap()
                .is_some_and(|m| m.has_body);
            if landed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the refresh job lands the message and its body");
    assert_eq!(repo::messages::count(&store).unwrap(), before + 1);
    ops.runner.stop();
}
