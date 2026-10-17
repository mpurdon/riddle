//! A pile of open Finds on one subject from one sender is a series
//! suggestion the Director can voice; once the route keeps a series
//! (`+ Replaces: same subject`) the suggestion goes, though the earlier
//! decisions still name the route's earlier text.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

const SENDER: &str = "getsupport@registrar.example";

fn notice(dig: &DigId, pid: &str, subject: &str, hours_ago: i64) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.date = chrono::SubsecRound::trunc_subsecs(chrono::Utc::now(), 3)
        - chrono::Duration::hours(hours_ago);
    m.subject = subject.into();
    m.from = Address::new(SENDER, None);
    m.to = vec![Address::new("me@example.com", None)];
    m.headers.message_id = Some(format!("<{pid}@registrar.example>"));
    m.headers.authentication.dkim = AuthVerdict::Pass;
    m
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pile_of_finds_on_one_subject_is_a_suggestion_until_the_route_keeps_a_series() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@example.com", 5, 5, json!({})).await;
    let mesh = common::install_mesh(
        &app,
        "Mesh: Renewals\nPriority: -58\nROUTES\nIF subject CONTAINS \"expires in\" → Find: Do",
        MeshStatus::Active,
    );
    let store = app.stores.dig(&dig.id).unwrap();
    for (pid, subject, hours_ago) in [
        ("a28", "a.com Expires in 28 days", 72),
        ("a14", "a.com Expires in 14 days", 48),
        ("a7", "Re: a.com Expires in 7 days", 24),
        ("b28", "b.com Expires in 28 days", 12),
    ] {
        repo::messages::upsert(&store, &notice(&dig.id, pid, subject, hours_ago)).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();

    // The fixture account has piles of its own (a repository's review
    // requests), so look for the registrar's.
    let ours = |all: Vec<SeriesSuggestion>| -> Option<SeriesSuggestion> {
        all.into_iter().find(|s| s.sender == SENDER)
    };
    let s = ours(app.series_suggestions(&[]).unwrap()).expect("a suggestion for the registrar");
    assert_eq!(s.mesh_id, mesh.id);
    assert_eq!((s.count, s.runs), (3, 1));
    assert_eq!(s.find_ids.len(), 3);
    assert!(s.text.contains("`+ Replaces: same subject`"), "{}", s.text);
    // Narrowed to another account, nothing.
    assert!(app
        .series_suggestions(&[DigId::from("dig-elsewhere")])
        .unwrap()
        .is_empty());

    // The route edited to keep a series: the old decisions still name the
    // route's earlier text, and the suggestion is gone.
    let text = "Mesh: Renewals\nPriority: -58\nROUTES\nIF subject CONTAINS \"expires in\" → Find: Do + Replaces: same subject";
    app.runtime
        .invoke_as_user("update_mesh", json!({ "meshId": mesh.id, "text": text }))
        .await
        .unwrap();
    let latest = repo::meshes::latest(&app.stores.index(), &mesh.id)
        .unwrap()
        .unwrap();
    assert!(latest.routes[0].output.replaces);
    assert!(ours(app.series_suggestions(&[]).unwrap()).is_none());
}
