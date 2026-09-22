//! Boot the whole app in memory with the scripted judge and let the job
//! runner do the work a click would trigger.

use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_settings::{JudgeProvider, LlmProvider, Settings};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_runs_catch_up_through_the_job_runner_and_the_task_centre_reports_it() {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "test",
    )))
    .unwrap();
    let mut s = Settings::dev_defaults();
    s.judge.provider = JudgeProvider::Scripted;
    s.llm.provider = LlmProvider::Null;
    let ops = app.boot_with(true, Some(s)).await.unwrap();
    app.runtime.arm(true);
    assert!(
        app.runtime.list_actions().len() > 40,
        "{} actions",
        app.runtime.list_actions().len()
    );

    let out = app
        .runtime
        .invoke_as_user(
            "add_fixture_dig",
            json!({ "name": "Work", "email": "matt@example.com", "seed": 5, "count": 60 }),
        )
        .await
        .unwrap();
    let dig: Dig = serde_json::from_value(out.result).unwrap();
    ops.queue
        .enqueue(riddle_jobs::JobSpec::new(JobKind::CatchUp, "Catch up Work").dig(dig.id.clone()))
        .unwrap();
    ops.runner.run_until_idle(Duration::from_secs(60)).await;

    let tc = app.task_centre().unwrap();
    assert_eq!(tc.overall, OverallStatus::CaughtUp, "{tc:?}");
    let row = tc.digs.iter().find(|d| d.dig_id == dig.id).unwrap();
    assert_eq!(row.status, DigStatus::Synced, "{:?}", row.status);
    let summary = app
        .summary(chrono::Utc::now() - chrono::Duration::days(90))
        .unwrap();
    assert!(summary.caught_up);
    assert!(summary.finds_open > 0);

    // Undo through the engine's Action: move one Find to Spoil, then undo it.
    let store = app.stores.dig(&dig.id).unwrap();
    let finds = riddle_store::repo::messages::list(
        &store,
        &dig.id,
        &riddle_store::repo::messages::MessageFilter {
            disposition: Some("find".into()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    let id = finds.items[0].id.clone();
    let moved = app
        .runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": [id] }),
        )
        .await
        .unwrap();
    let undone = app
        .runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": moved.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    let report: UndoReport = serde_json::from_value(undone.result).unwrap();
    assert_eq!(report.restored.len(), 1, "{report:?}");
    let back = riddle_store::repo::classifications::current(&store, &id)
        .unwrap()
        .unwrap();
    assert!(matches!(back.disposition, Disposition::Find { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sift_pass_leaves_activity_with_provenance_and_undo_all_returns_messages_to_unresolved() {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "test",
    )))
    .unwrap();
    let mut s = Settings::dev_defaults();
    s.judge.provider = JudgeProvider::Scripted;
    s.llm.provider = LlmProvider::Null;
    let ops = app.boot_with(true, Some(s)).await.unwrap();
    app.runtime.arm(true);
    let out = app
        .runtime
        .invoke_as_user(
            "add_fixture_dig",
            json!({ "name": "Work", "email": "matt@example.com", "seed": 8, "count": 60 }),
        )
        .await
        .unwrap();
    let dig: Dig = serde_json::from_value(out.result).unwrap();
    ops.queue
        .enqueue(riddle_jobs::JobSpec::new(JobKind::CatchUp, "Catch up Work").dig(dig.id.clone()))
        .unwrap();
    ops.runner.run_until_idle(Duration::from_secs(60)).await;

    let page = app
        .list_activity(
            &riddle_store::repo::activities::ActivityFilter {
                actor: Some(Actor::Sift),
                ..Default::default()
            },
            &PageRequest::default(),
        )
        .unwrap();
    assert!(!page.items.is_empty(), "Sift left Activity");
    let spoil = page
        .items
        .iter()
        .find(|a| a.summary.contains("to Spoil"))
        .expect("a Spoil entry");
    let prov = spoil.provenance.as_ref().unwrap();
    assert!(prov.mesh_name.is_some());
    assert!(
        prov.note.as_ref().unwrap().contains("evaluation")
            || prov.note.as_ref().unwrap().contains("cached")
    );
    assert!(matches!(spoil.undo, UndoInfo::Undoable { .. }));
    let n = spoil.affected.len();

    let undone = app
        .runtime
        .invoke_as_user("undo_activity", json!({ "activityId": spoil.id }))
        .await
        .unwrap();
    let report: UndoReport = serde_json::from_value(undone.result).unwrap();
    assert_eq!(report.restored.len(), n, "{report:?}");
    let store = app.stores.dig(&dig.id).unwrap();
    for e in &spoil.affected {
        let k =
            riddle_store::repo::classifications::current(&store, &MessageId::from(e.id.as_str()))
                .unwrap()
                .unwrap();
        assert_eq!(k.disposition, Disposition::Unresolved);
    }
}
