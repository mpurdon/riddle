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

/// A Director's proposal outlives the pending list: applied, declined and
/// replaced proposals stay readable with their text, and a newer proposal
/// for the same Mesh from the same conversation replaces the older card
/// (two Payments drafts, both "v6 → v7", waited side by side).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proposals_keep_their_history_and_a_newer_one_replaces_the_older() {
    let (app, _ops) = booted().await;
    let index = app.stores.index();
    let mesh = common::install_mesh(
        &app,
        "Mesh: Bills\nPriority: 77\nROUTES\nIF invoice@v1 > .80\n    → Find: Pay",
        MeshStatus::Active,
    );
    let director = |conv: &str| Principal::Director {
        conversation_id: conv.into(),
        read_untrusted: true,
    };
    let propose = |text: &str, conv: &str| ActionInvocation {
        action: "update_mesh".into(),
        args: json!({ "meshId": mesh.id, "text": text }),
        principal: director(conv),
        dig_id: None,
        idempotency_key: None,
        confirmed: false,
        pending_id: None,
    };
    let pending_of = |e: riddle_errors::RiddleError| -> PendingAction {
        serde_json::from_value(e.detail.unwrap()).unwrap()
    };
    let first = pending_of(
        app.runtime
            .invoke(propose(
                "Mesh: Bills\nPriority: 77\nROUTES\nIF invoice@v1 > .85\n    → Find: Pay",
                "conv_a",
            ))
            .await
            .unwrap_err(),
    );
    let other = pending_of(
        app.runtime
            .invoke(propose(
                "Mesh: Bills\nPriority: 77\nROUTES\nIF invoice@v1 > .95\n    → Find: Pay",
                "conv_b",
            ))
            .await
            .unwrap_err(),
    );
    let second = pending_of(
        app.runtime
            .invoke(propose(
                "Mesh: Bills\nPriority: 77\nROUTES\nIF invoice@v1 > .90\n    → Find: Pay",
                "conv_a",
            ))
            .await
            .unwrap_err(),
    );

    let open: Vec<String> = repo::pending::list_pending(&index)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert!(
        !open.contains(&first.id),
        "the same conversation's older proposal is replaced"
    );
    assert!(
        open.contains(&second.id) && open.contains(&other.id),
        "another conversation's stands"
    );

    // Apply the newer one, decline the other conversation's.
    app.runtime
        .invoke(ActionInvocation {
            args: second.args.clone(),
            principal: Principal::User,
            confirmed: true,
            pending_id: Some(second.id.clone()),
            ..propose("", "conv_a")
        })
        .await
        .unwrap();
    app.runtime
        .invoke_as_user(
            "dismiss_pending",
            json!({ "pendingId": other.id, "reason": "too strict" }),
        )
        .await
        .unwrap();

    let history = repo::pending::proposals(&index, Some("conv_a"), 10).unwrap();
    let outcome = |id: &str| {
        history
            .iter()
            .find(|r| r.proposal.id == id)
            .map(|r| (r.outcome, r.note.clone()))
    };
    assert_eq!(
        outcome(&first.id),
        Some((ProposalOutcome::Replaced, Some(second.id.clone())))
    );
    let applied = history.iter().find(|r| r.proposal.id == second.id).unwrap();
    assert_eq!(applied.outcome, ProposalOutcome::Applied);
    assert!(applied.activity_id.is_some());
    assert!(
        applied.proposal.args["text"]
            .as_str()
            .unwrap()
            .contains(".90"),
        "the text is kept"
    );
    // Activity reads the same story by conversation: the Director's proposals, and the change applied from one.
    let acts = app
        .list_activity(
            &repo::activities::ActivityFilter {
                conversation_id: Some("conv_a".into()),
                ..Default::default()
            },
            &PageRequest::default(),
        )
        .unwrap()
        .items;
    let proposed = acts
        .iter()
        .filter(|a| a.actor == Actor::Director && a.summary.starts_with("Proposed:"))
        .count();
    assert_eq!(proposed, 2, "{acts:#?}");
    let applied = acts
        .iter()
        .find(|a| a.action == "update_mesh")
        .expect("the applied change");
    assert_eq!(
        (applied.actor, applied.why.as_deref()),
        (Actor::User, Some("Proposed by the Director"))
    );
    let b = repo::pending::proposals(&index, Some("conv_b"), 10).unwrap();
    assert_eq!(
        (b[0].outcome, b[0].note.as_deref()),
        (ProposalOutcome::Declined, Some("too strict"))
    );
}
