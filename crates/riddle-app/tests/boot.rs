//! Boot the whole app in memory with the scripted judge and let the job
//! runner do the work a click would trigger.

use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_settings::{JudgeProvider, LlmProvider, Settings};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_runs_catch_up_through_the_job_runner_and_the_task_centre_reports_it() {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars("0", None, None, None, "test"))).unwrap();
    let mut s = Settings::dev_defaults();
    s.judge.provider = JudgeProvider::Scripted;
    s.llm.provider = LlmProvider::Null;
    let ops = app.boot_with(true, Some(s)).await.unwrap();
    app.runtime.arm(true);
    assert!(app.runtime.list_actions().len() > 40, "{} actions", app.runtime.list_actions().len());

    let out = app.runtime.invoke_as_user("add_fixture_dig", json!({ "name": "Work", "email": "matt@example.com", "seed": 5, "count": 60 })).await.unwrap();
    let dig: Dig = serde_json::from_value(out.result).unwrap();
    ops.queue.enqueue(riddle_jobs::JobSpec::new(JobKind::CatchUp, "Catch up Work").dig(dig.id.clone())).unwrap();
    ops.runner.run_until_idle(Duration::from_secs(60)).await;

    let tc = app.task_centre().unwrap();
    assert_eq!(tc.overall, OverallStatus::CaughtUp, "{tc:?}");
    let row = tc.digs.iter().find(|d| d.dig_id == dig.id).unwrap();
    assert_eq!(row.status, DigStatus::Synced, "{:?}", row.status);
    let summary = app.summary(chrono::Utc::now() - chrono::Duration::days(90)).unwrap();
    assert!(summary.caught_up);
    assert!(summary.finds_open > 0);

    // Undo through the engine's Action: move one Find to Spoil, then undo it.
    let store = app.stores.dig(&dig.id).unwrap();
    let finds = riddle_store::repo::messages::list(&store, &dig.id, &riddle_store::repo::messages::MessageFilter { disposition: Some("find".into()), ..Default::default() }, &PageRequest::default()).unwrap();
    let id = finds.items[0].id.clone();
    let moved = app.runtime.invoke_as_user("move_to_spoil", json!({ "digId": dig.id, "messageIds": [id] })).await.unwrap();
    let undone = app.runtime.invoke_as_user("undo_activity", json!({ "activityId": moved.activity_id.unwrap() })).await.unwrap();
    let report: UndoReport = serde_json::from_value(undone.result).unwrap();
    assert_eq!(report.restored.len(), 1, "{report:?}");
    let back = riddle_store::repo::classifications::current(&store, &id).unwrap().unwrap();
    assert!(matches!(back.disposition, Disposition::Find { .. }));
}
