//! Boot the whole app in memory with the scripted judge and let the job
//! runner do the work a click would trigger.

use riddle_core::*;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_runs_catch_up_through_the_job_runner_and_the_task_centre_reports_it() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    assert!(
        app.runtime.list_actions().len() > 40,
        "{} actions",
        app.runtime.list_actions().len()
    );

    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "matt@example.com", 5, 60, json!({})).await;

    let tc = app.task_centre().unwrap();
    assert_eq!(tc.overall, OverallStatus::CaughtUp, "{tc:?}");
    let row = tc.digs.iter().find(|d| d.dig_id == dig.id).unwrap();
    assert_eq!(row.status, DigStatus::Synced, "{:?}", row.status);
    let summary = app
        .summary(chrono::Utc::now() - chrono::Duration::days(90), None)
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
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "matt@example.com", 8, 60, json!({})).await;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn boot_records_the_system_timezone_so_end_of_day_is_the_user_s() {
    let (app, _ops) = common::boot_scripted(false, |_| {}).await;
    let tz = riddle_store::repo::user_profile::get(&app.stores.index())
        .unwrap()
        .timezone;
    assert_eq!(tz, iana_time_zone::get_timezone().ok());
    assert!(
        tz.as_deref()
            .is_some_and(|t| t.parse::<chrono_tz::Tz>().is_ok()),
        "a zone the resolver can read: {tz:?}"
    );
}
