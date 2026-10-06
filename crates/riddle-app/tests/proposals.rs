//! Declining a Director proposal reaches the store, and the Summary can be
//! asked about one account. Both came out of the UX study: proposals that
//! were dismissed in the client came back on the next launch because the
//! PendingAction stayed in the store, and the rail badges contradicted the
//! pages because the Summary always counted every account.

use riddle_app::App;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::time::Duration;

mod common;

async fn booted() -> (std::sync::Arc<App>, std::sync::Arc<riddle_app::ops::Ops>) {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    (app, ops)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn declining_a_proposal_removes_it_from_the_pending_list_and_records_it_in_activity() {
    let (app, _ops) = booted().await;
    let index = app.stores.index();
    let pending = PendingAction {
        id: "pend_1".into(),
        action: "create_mesh".into(),
        args: json!({ "text": "mesh \"GitHub CI\" v1" }),
        real_arguments: json!({ "text": "mesh \"GitHub CI\" v1" }),
        summary: "Create Mesh GitHub CI".into(),
        confirmation: ConfirmationPolicy::Approval,
        reason: "the Director read untrusted content".into(),
        principal: Principal::Director {
            conversation_id: "conv_1".into(),
            read_untrusted: true,
        },
        created_at: chrono::Utc::now(),
    };
    repo::pending::put_pending(&index, &pending).unwrap();
    assert_eq!(repo::pending::list_pending(&index).unwrap().len(), 1);

    let out = app
        .runtime
        .invoke_as_user(
            "dismiss_pending",
            json!({ "pendingId": "pend_1", "reason": "GitHub failures should stay in Unresolved for now" }),
        )
        .await
        .unwrap();
    assert!(repo::pending::list_pending(&index).unwrap().is_empty());
    let activity = repo::activities::get(&index, &out.activity_id.unwrap())
        .unwrap()
        .unwrap();
    assert!(
        activity
            .summary
            .starts_with("Declined: Create Mesh GitHub CI"),
        "{}",
        activity.summary
    );
    assert_eq!(
        activity.why.as_deref(),
        Some("GitHub failures should stay in Unresolved for now")
    );
    assert!(matches!(activity.undo, UndoInfo::NotUndoable { .. }));

    // Declining twice says so instead of failing silently.
    let again = app
        .runtime
        .invoke_as_user("dismiss_pending", json!({ "pendingId": "pend_1" }))
        .await
        .unwrap_err();
    assert_eq!(again.kind, riddle_errors::ErrorKind::NotFound, "{again:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_summary_for_one_account_counts_only_that_account() {
    let (app, ops) = booted().await;
    let mut digs = Vec::new();
    for (name, seed) in [("Work", 5u64), ("Personal", 9u64)] {
        let out = app
            .runtime
            .invoke_as_user(
                "add_fixture_dig",
                json!({ "name": name, "email": format!("{}@example.com", name.to_lowercase()), "seed": seed, "count": 40 }),
            )
            .await
            .unwrap();
        let dig: Dig = serde_json::from_value(out.result).unwrap();
        ops.queue
            .enqueue(
                riddle_jobs::JobSpec::new(JobKind::CatchUp, format!("Catch up {name}"))
                    .dig(dig.id.clone()),
            )
            .unwrap();
        digs.push(dig);
    }
    ops.runner.run_until_idle(Duration::from_secs(60)).await;

    let since = chrono::Utc::now() - chrono::Duration::days(90);
    let all = app.summary(since, None).unwrap();
    let work = app.summary(since, Some(&digs[0].id)).unwrap();
    let personal = app.summary(since, Some(&digs[1].id)).unwrap();
    assert_eq!(all.dig_count, 2);
    assert_eq!(work.dig_count, 1);
    assert_eq!(work.arrived + personal.arrived, all.arrived);
    assert_eq!(work.finds_open + personal.finds_open, all.finds_open);
    assert!(work.arrived > 0 && personal.arrived > 0);
}
