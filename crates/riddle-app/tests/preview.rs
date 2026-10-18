//! A decision previewed, not made; the walk reading what the decision saw;
//! and where a pass id goes (17 Oct 2026). Before this, learning what a
//! message would be decided now meant re-deciding it for real, the walk
//! replayed the pinned Mesh against today's lists, and a pass's decisions
//! could not be read together.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

/// A new, unclassified message in the Dig from a shop, so a pass has
/// something to decide and a shop rule something to take.
fn fresh_message(app: &riddle_app::App, dig: &Dig, id: &str) -> MessageId {
    let store = app.stores.dig(&dig.id).unwrap();
    let mut m = riddle_mail::fixture::blank_message(dig.id.as_str(), id);
    m.from = Address::new(format!("{id}@shop.example"), None);
    m.subject = format!("Order {id} has shipped");
    m.body_text = Some("Your order is on its way.".into());
    m.has_body = true;
    repo::messages::upsert(&store, &m).unwrap();
    m.id
}

fn shops_list(app: &riddle_app::App, entries: &[&str]) {
    repo::lists::put(
        &app.stores.index(),
        &NamedListDef {
            name: "shops".into(),
            description: String::new(),
            entries: entries.iter().map(|e| e.to_string()).collect(),
            pack: None,
            kinds: vec![],
        },
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_reports_the_would_be_decision_and_leaves_the_stored_decision_and_trace_untouched(
) {
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let id = fresh_message(&app, &dig, "prev1");
    app.sift_pass(&dig, 100, None).await.unwrap();
    let before = repo::classifications::current(&store, &id)
        .unwrap()
        .expect("decided by the pass");
    assert_eq!(repo::traces::for_message(&store, &id).unwrap().len(), 1);

    // A rule that would take it now, first in the order.
    common::install_mesh(
        &app,
        "Mesh: Shop mail\nScope:\nAll Digs\nPriority: -50\n\nROUTES\n\nIF sender_domain = shop.example → Filed + Stratum: Shopping",
        MeshStatus::Active,
    );
    let p = app.decide_preview(&dig.id, &id).await.unwrap();
    assert_eq!(
        p.stored.as_ref().map(|k| k.id.clone()),
        Some(before.id.clone())
    );
    assert_eq!(p.would.disposition, Disposition::Filed, "{}", p.diff);
    assert_eq!(p.would_trace.mesh_name.as_deref(), Some("Shop mail"));
    assert!(p.changed, "{}", p.diff);
    // "would be Filed by Shop mail v1 instead of …", or "still Filed, but
    // by Shop mail v1 instead of …" when the fixture rules had filed it.
    assert!(p.diff.contains("by Shop mail v1 instead of"), "{}", p.diff);
    assert!(p.would_trace.pass_id.starts_with("preview-"));
    assert!(p.would_trace.policy.is_some());

    // Nothing was written: the classification, its history and its trace stand.
    assert_eq!(
        repo::classifications::current(&store, &id)
            .unwrap()
            .unwrap()
            .id,
        before.id
    );
    assert_eq!(
        repo::classifications::history(&store, &id).unwrap().len(),
        1
    );
    assert_eq!(repo::traces::for_message(&store, &id).unwrap().len(), 1);

    // The router takes a pasted link whole, like `explain`.
    let link = format!("riddle://message/{}/{} (Sender · Subject)", dig.id, id);
    let v = riddle_app::dispatch::call(&app, "decide_preview", json!({ "messageId": link }))
        .await
        .unwrap();
    assert_eq!(v["changed"], true);
    assert_eq!(v["would"]["disposition"]["kind"], "filed");
    assert_eq!(repo::traces::for_message(&store, &id).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_walk_reads_the_traces_list_snapshot_and_does_not_change_when_the_list_is_edited_afterwards(
) {
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    shops_list(&app, &["shop.example"]);
    common::install_mesh(
        &app,
        "Mesh: Shops\nScope:\nAll Digs\nPriority: -50\n\nROUTES\n\nIF sender_domain IN list:shops → Filed + Stratum: Shopping",
        MeshStatus::Active,
    );
    let id = fresh_message(&app, &dig, "walk1");
    app.sift_pass(&dig, 100, None).await.unwrap();
    let ex = app.explain(&dig.id, &id).unwrap();
    assert_eq!(ex.mesh_name.as_deref(), Some("Shops"), "{}", ex.reason);
    let clause = &ex.routes[0].clauses[0];
    assert_eq!(clause.text, "sender_domain IN list:shops");
    assert_eq!(clause.result, ClauseResult::Pass);
    assert_eq!(ex.routes[0].outcome, RouteOutcome::Matched);
    let t = app.read_trace(&dig.id, &ex.trace_id).unwrap();
    assert!(
        t.list_results
            .iter()
            .any(|l| l.list == "list:shops" && l.matched),
        "{:?}",
        t.list_results
    );
    assert!(t.policy.is_some());
    assert!(t
        .mesh_outcomes
        .iter()
        .any(|o| o.name == "Shops" && matches!(o.outcome, MeshOutcomeKind::Matched { .. })));
    // The deciding Mesh is not among the "earlier" lines; the guards Mesh is.
    assert!(ex.earlier_meshes.iter().all(|l| !l.starts_with("Shops v1")));
    assert!(
        ex.earlier_meshes
            .iter()
            .any(|l| l == "System guards v1: no route matched"),
        "{:?}",
        ex.earlier_meshes
    );

    // The shop leaves the list: the stored decision's walk still says what it saw.
    shops_list(&app, &[]);
    let again = app.explain(&dig.id, &id).unwrap();
    assert_eq!(again.routes[0].clauses[0].result, ClauseResult::Pass);
    assert_eq!(again.routes[0].outcome, RouteOutcome::Matched);
    // A new decision reads today's list, and says so on its own trace.
    let p = app.decide_preview(&dig.id, &id).await.unwrap();
    assert_ne!(
        p.would_trace.mesh_name.as_deref(),
        Some("Shops"),
        "{}",
        p.diff
    );
    assert!(p
        .would_trace
        .list_results
        .iter()
        .any(|l| l.list == "list:shops" && !l.matched));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pass_id_is_on_the_trace_the_log_payload_and_the_activity_inputs() {
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let id = fresh_message(&app, &dig, "pass1");
    let report = app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(report.processed, 1, "{report:?}");
    assert!(!report.pass_id.is_empty());
    let k = repo::classifications::current(&store, &id)
        .unwrap()
        .unwrap();
    let t = repo::traces::get(&store, &k.trace_id).unwrap().unwrap();
    assert_eq!(t.pass_id, report.pass_id);
    let lines = riddle_observability::tail(2000, Some("sift"), None, Some(&report.pass_id));
    assert!(
        lines.iter().any(|l| l
            .payload
            .as_ref()
            .is_some_and(|p| p["passId"] == report.pass_id && p["messageId"] == id.as_str())),
        "{:?}",
        lines.iter().map(|l| l.message.clone()).collect::<Vec<_>>()
    );
    let page = app
        .list_activity(
            &repo::activities::ActivityFilter {
                dig_id: Some(dig.id.to_string()),
                action: Some("sift_classify".into()),
                ..Default::default()
            },
            &PageRequest {
                cursor: None,
                limit: Some(20),
            },
        )
        .unwrap();
    assert!(
        page.items
            .iter()
            .any(|a| a.inputs["passId"] == report.pass_id),
        "{:?}",
        page.items
            .iter()
            .map(|a| a.inputs.clone())
            .collect::<Vec<_>>()
    );
}
