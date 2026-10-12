//! The Director reads Meshes as an index, or one Mesh in full: every Mesh
//! with its text was cut short by the tool loop's limit on a real profile,
//! and the Director proposed a Payments version built from one route it had
//! seen, dropping a route and two guards.

use riddle_agent::reads::{DirectorReads, ReadScope};
use riddle_app::director_reads::AppReads;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_director_lists_meshes_as_an_index_and_reads_one_in_full() {
    let (app, _ops) = common::boot_scripted(false, |_| {}).await;
    common::install_mesh(
        &app,
        "Mesh: Invoice check\nROUTES\nIF invoice@v1 > .80 AND marketing@v1 < .90\n    → Find: Pay",
        riddle_core::MeshStatus::Active,
    );
    let reads = AppReads(app.clone());
    let scope = ReadScope::default();
    let index = reads.list_meshes(&scope, json!({})).await.unwrap();
    let rows = index.as_array().unwrap();
    assert!(!rows.is_empty());
    for r in rows {
        assert!(r.get("name").is_some() && r.get("version").is_some());
        assert!(
            r.get("sourceText").is_none() && r.get("routes").is_none(),
            "the index carries no text: {r}"
        );
    }
    let one = reads
        .list_meshes(&scope, json!({ "meshId": "invoice CHECK" }))
        .await
        .unwrap();
    let text = one["sourceText"].as_str().unwrap();
    assert!(
        text.contains("marketing@v1 < .90"),
        "the whole current text: {text}"
    );
    assert!(reads
        .list_meshes(&scope, json!({ "meshId": "No such Mesh" }))
        .await
        .is_err());
}
