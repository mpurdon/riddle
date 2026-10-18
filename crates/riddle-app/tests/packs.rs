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

/// A received message from `sender`, dated now, in the Dig's store.
fn received(app: &App, dig: &Dig, pid: &str, sender: &str) {
    let store = app.stores.dig(&dig.id).unwrap();
    let mut m = riddle_mail::fixture::blank_message(dig.id.as_str(), pid);
    m.from = Address::new(sender, None);
    m.subject = format!("Message {pid}");
    repo::messages::upsert(&store, &m).unwrap();
}

fn offered(app: &App) -> Vec<String> {
    app.pack_offers(&[])
        .unwrap()
        .into_iter()
        .map(|o| o.pack_id)
        .collect()
}

fn installed(app: &App) -> Vec<String> {
    riddle_mesh::packs::installed(&app.stores.index())
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect()
}

fn activities_named(app: &App, action: &str) -> Vec<Activity> {
    repo::activities::list(
        &app.stores.index(),
        &Default::default(),
        &Default::default(),
    )
    .unwrap()
    .items
    .into_iter()
    .filter(|a| a.action == action)
    .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_listing_turns_on_the_always_packs_active_under_the_system_actor_once() {
    let (app, ops) = common::boot_scripted(false, |s| s.install_packs_on_first_sync = true).await;
    assert!(installed(&app).is_empty(), "a fresh store has no packs");
    common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    let on = installed(&app);
    assert!(
        on.contains(&"core".to_string()) && on.contains(&"accounts".to_string()),
        "{on:?}"
    );
    // Active, not Shadow: a new account's rules decide.
    let meshes = repo::meshes::list_latest(&app.stores.index(), false).unwrap();
    let sign_in = meshes
        .iter()
        .find(|m| m.name == "Sign-in and verification")
        .expect("the Accounts pack's Mesh");
    assert_eq!(sign_in.status, MeshStatus::Active);
    assert_eq!(
        sign_in.pack.as_ref().map(|p| p.id.as_str()),
        Some("accounts")
    );
    // Never offered, and said once in Activity under System.
    assert!(offered(&app)
        .iter()
        .all(|id| id != "core" && id != "accounts"));
    let rows = activities_named(&app, "install_detected_packs");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].actor, Actor::System);
    assert!(rows[0].summary.contains("core"), "{}", rows[0].summary);
    // A second account's first listing has nothing left to install.
    common::fixture_dig_synced(&app, &ops, "Home", "me@home.example", 4, 8, json!({})).await;
    assert_eq!(activities_named(&app, "install_detected_packs").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_test_boot_leaves_the_always_packs_off_unless_it_turns_them_on() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    assert!(installed(&app).is_empty());
    assert!(activities_named(&app, "install_detected_packs").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn install_detected_packs_turns_on_the_always_packs_active_and_undo_takes_them_off() {
    let (app, _ops) = common::boot_scripted(false, |_| {}).await;
    let out = invoke(&app, "install_detected_packs", json!({})).await;
    let on = installed(&app);
    assert!(
        on.contains(&"core".to_string()) && on.contains(&"accounts".to_string()),
        "{on:?}"
    );
    let meshes = repo::meshes::list_latest(&app.stores.index(), false).unwrap();
    assert!(meshes
        .iter()
        .any(|m| m.name == "Sign-in and verification" && m.status == MeshStatus::Active));
    let again = invoke(&app, "install_detected_packs", json!({})).await;
    assert!(again.summary.contains("already on"), "{}", again.summary);
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": out.activity_id.expect("recorded") }),
        )
        .await
        .unwrap();
    assert!(installed(&app).is_empty(), "undo takes the packs off");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_offer_goes_away_when_its_pack_is_turned_on_or_declined_and_undo_offers_it_again() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    for i in 0..10 {
        received(&app, &dig, &format!("gh{i}"), "notifications@github.com");
    }
    received(&app, &dig, "cra", "noreply@canada.ca");
    let before = offered(&app);
    assert!(
        before.contains(&"developer-github".to_string()),
        "{before:?}"
    );
    assert!(
        before.contains(&"government-canada".to_string()),
        "{before:?}"
    );
    // The Summary carries the offers, worded with the count.
    let summary = app
        .summary(chrono::Utc::now() - chrono::Duration::hours(24), None)
        .unwrap();
    let github = summary
        .pack_offers
        .iter()
        .find(|o| o.pack_id == "developer-github")
        .expect("GitHub offered on the Summary");
    // The fixture mailbox has GitHub mail of its own; the sentence carries
    // whatever the count is.
    assert!(github.matched_messages >= 10, "{github:?}");
    assert_eq!(
        github.reason_in_words,
        format!(
            "Riddle recognised GitHub mail ({} messages). Turn its rules on?",
            github.matched_messages
        )
    );

    // Not now: declined, and offered again when the decline is undone.
    let declined = invoke(
        &app,
        "dismiss_pack_offer",
        json!({ "packId": "government-canada" }),
    )
    .await;
    assert!(!offered(&app).contains(&"government-canada".to_string()));
    // Turn on: installed Active through install_pack, and no longer offered.
    invoke(
        &app,
        "install_pack",
        json!({ "packId": "developer-github", "status": "active", "replace": true }),
    )
    .await;
    assert!(!offered(&app).contains(&"developer-github".to_string()));
    assert!(installed(&app).contains(&"developer-github".to_string()));
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": declined.activity_id.expect("recorded") }),
        )
        .await
        .unwrap();
    assert!(offered(&app).contains(&"government-canada".to_string()));
}
