//! Applying a changed Mesh to past mail. A new Mesh version only decides new
//! mail; `apply_mesh_to_history` sends the messages it would decide
//! differently back to Sift, leaves the user's own decisions alone, and
//! undoes cleanly.

use riddle_app::ops::Ops;
use riddle_app::App;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::sync::Arc;

mod common;

const MESH: &str = "mesh_human_correspondence";

async fn sifted() -> (Arc<App>, Arc<Ops>, Dig) {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 80, json!({})).await;
    (app, ops, dig)
}

async fn invoke(app: &App, action: &str, args: serde_json::Value) -> ActionOutcome {
    app.runtime
        .invoke(ActionInvocation {
            action: action.into(),
            args,
            principal: Principal::User,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
        .unwrap()
}

fn decided_by_mesh(app: &App, dig: &Dig) -> Vec<MessageId> {
    let store = app.stores.dig(&dig.id).unwrap();
    repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        1000,
    )
    .unwrap()
    .into_iter()
    .filter(|id| {
        repo::classifications::current(&store, id)
            .unwrap()
            .is_some_and(|k| k.mesh_id.as_ref().map(|m| m.as_str()) == Some(MESH))
    })
    .collect()
}

/// Change "Human correspondence" so replies it would have raised as Finds
/// are Filed instead, and make that version the running one.
async fn file_instead_of_respond(app: &App) {
    let index = app.stores.index();
    let current = repo::meshes::latest(&index, &MeshId(MESH.into()))
        .unwrap()
        .unwrap();
    let text = current.source_text.replace(
        "→ Find: Respond + Stratum: People",
        "→ Filed + Stratum: People",
    );
    assert_ne!(text, current.source_text);
    invoke(app, "update_mesh", json!({ "meshId": MESH, "text": text })).await;
    let latest = repo::meshes::latest(&index, &MeshId(MESH.into()))
        .unwrap()
        .unwrap();
    invoke(
        app,
        "set_mesh_status",
        json!({ "meshId": MESH, "version": latest.version, "status": "active" }),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn applying_a_changed_mesh_redecides_past_mail_but_not_your_own_decisions() {
    let (app, ops, dig) = sifted().await;
    let decided = decided_by_mesh(&app, &dig);
    assert!(
        decided.len() >= 2,
        "the fixture has Human correspondence decisions"
    );
    let store = app.stores.dig(&dig.id).unwrap();
    let respond: Vec<MessageId> = decided
        .iter()
        .filter(|id| {
            matches!(
                repo::classifications::current(&store, id)
                    .unwrap()
                    .unwrap()
                    .disposition,
                Disposition::Find {
                    find_type: FindType::Respond
                }
            )
        })
        .cloned()
        .collect();
    assert!(
        respond.len() >= 2,
        "some were Respond Finds: {}",
        respond.len()
    );

    // One of them you decide yourself; a rule change must leave it alone.
    let yours = respond[0].clone();
    app.runtime.arm(true);
    invoke(
        &app,
        "set_disposition",
        json!({ "digId": dig.id, "messageId": yours, "disposition": "find", "findType": "respond" }),
    )
    .await;

    file_instead_of_respond(&app).await;
    let unclassified_before = repo::messages::unclassified_count(&store).unwrap();
    let out = invoke(
        &app,
        "apply_mesh_to_history",
        json!({ "meshId": MESH, "windowDays": 3650 }),
    )
    .await;
    let count = out.result["count"].as_u64().unwrap();
    assert!(count >= 1, "{}", out.result);
    assert!(out.result["keptYours"].as_u64().unwrap() >= 1);
    // The Sift pass starts at once, so the backlog is checked on the rows
    // themselves: every re-decided message has its prior decision superseded.
    let activity = repo::activities::get(&app.stores.index(), &out.activity_id.unwrap())
        .unwrap()
        .unwrap();
    let UndoInfo::Undoable { restore, .. } = activity.undo else {
        panic!("apply is undoable")
    };
    let sent = restore["messages"].as_array().unwrap();
    assert_eq!(sent.len() as u64, count);
    for entry in sent {
        let id = MessageId(entry["messageId"].as_str().unwrap().to_string());
        let prior_id = entry["prior"]["id"].as_str().unwrap();
        // The prior row stays in history and is no longer the current decision
        // (Sift may already have made a fresh one, or not yet).
        assert!(
            repo::classifications::history(&store, &id)
                .unwrap()
                .iter()
                .any(|k| k.id.as_str() == prior_id),
            "the prior decision of {id} stays in history"
        );
        assert_ne!(
            repo::classifications::current(&store, &id)
                .unwrap()
                .map(|k| k.id.as_str().to_string()),
            Some(prior_id.to_string()),
            "the prior decision of {id} is no longer current"
        );
    }
    let kept = repo::classifications::current(&store, &yours)
        .unwrap()
        .unwrap();
    assert_eq!(kept.mechanism, DecisionMechanism::User);
    ops.runner
        .run_until_idle(std::time::Duration::from_secs(60))
        .await;

    // Undo puts every prior decision back.
    app.runtime
        .undo_via(
            "apply_mesh_to_history",
            restore,
            Principal::User,
            None,
            ActivityId::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        repo::messages::unclassified_count(&store).unwrap(),
        unclassified_before
    );
    let back = repo::classifications::current(&store, &respond[1])
        .unwrap()
        .unwrap();
    assert!(matches!(
        back.disposition,
        Disposition::Find {
            find_type: FindType::Respond
        }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mesh_that_is_not_active_cannot_be_applied_to_past_mail() {
    let (app, _ops, _dig) = sifted().await;
    let index = app.stores.index();
    let current = repo::meshes::latest(&index, &MeshId(MESH.into()))
        .unwrap()
        .unwrap();
    invoke(
        &app,
        "update_mesh",
        json!({ "meshId": MESH, "text": current.source_text }),
    )
    .await;
    let err = app
        .runtime
        .invoke(ActionInvocation {
            action: "apply_mesh_to_history".into(),
            args: json!({ "meshId": MESH }),
            principal: Principal::User,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind.as_str(), "invalid");
    assert!(err.message.contains("activate"), "{}", err.message);
}
