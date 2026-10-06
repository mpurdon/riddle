//! Rule packs through the Actions the app calls: a new person installs
//! Core and Shopping with an answer for Shopping's list, the Meshes land in
//! their bands, a store named in the list keeps its promotions, a shared
//! pack file round-trips through export and import, and removing a pack
//! takes its Meshes away and leaves the person's lists.

use riddle_app::App;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_person_installs_packs_answers_their_lists_shares_one_and_removes_one() {
    let (app, _ops) = common::boot_scripted(false, |_| {}).await;
    let index = app.stores.index();
    // A fresh store already has the default Meshes; Core takes them over.
    invoke(
        &app,
        "install_pack",
        json!({ "packId": "core", "replace": true, "status": "active" }),
    )
    .await;
    invoke(
        &app,
        "install_pack",
        json!({ "packId": "shopping", "replace": true, "lists": { "keep_promotions": ["@Uber.com"] } }),
    )
    .await;

    let catalogue = riddle_mesh::packs::catalogue(&index).unwrap();
    let shopping = catalogue.iter().find(|p| p.pack.id == "shopping").unwrap();
    assert_eq!(
        shopping.installed.as_ref().unwrap().version,
        shopping.pack.version
    );
    assert_eq!(shopping.list_entries["keep_promotions"], vec!["uber.com"]);

    let running = repo::meshes::list_running(&index).unwrap();
    let at = |name: &str| {
        running
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.priority)
            .unwrap()
    };
    assert!(
        at("System guards") < at("Human correspondence"),
        "safety before personal"
    );
    assert!(
        at("Human correspondence") < at("Security and account alerts"),
        "personal before specific"
    );
    assert!(
        at("Frequent promotions") < at("Account notices"),
        "general before fallback"
    );
    let mut priorities: Vec<i32> = running.iter().map(|m| m.priority).collect();
    priorities.sort();
    let before = priorities.len();
    priorities.dedup();
    assert_eq!(
        priorities.len(),
        before,
        "no two running Meshes share a Priority"
    );

    // Share: export, then import under another id.
    let exported = invoke(&app, "export_pack", json!({ "packId": "shopping" })).await;
    let text = exported.result["text"]
        .as_str()
        .unwrap()
        .replace("id = \"shopping\"", "id = \"shopping-copy\"");
    invoke(&app, "import_pack", json!({ "text": text })).await;
    assert!(riddle_mesh::packs::catalogue(&index)
        .unwrap()
        .iter()
        .any(|p| p.pack.id == "shopping-copy" && !p.builtin));

    // Remove: its Meshes go, the list stays.
    invoke(&app, "remove_pack", json!({ "packId": "shopping" })).await;
    let running = repo::meshes::list_running(&index).unwrap();
    assert!(!running.iter().any(|m| m.name == "Frequent promotions"));
    assert!(repo::lists::get(&index, "keep_promotions")
        .unwrap()
        .is_some());
}
