//! Every read the Director has reaches the model as valid JSON within the
//! tool loop's limit, and says what it left out. Results over the limit
//! used to be cut mid-JSON before `total` and `nextCursor`: on a real
//! profile the Director saw 15 of 50 Activity rows, 22 of 181 open Finds,
//! and none of the Mesh it was editing (12 Oct 2026). A new read tool joins
//! this list, so it cannot bring the cut back.

use riddle_agent::fit::fit;
use riddle_agent::reads::{DirectorReads, ReadScope};
use riddle_agent::tool_loop::MAX_RESULT_CHARS;
use riddle_app::director_reads::AppReads;
use riddle_store::repo;
use serde_json::{json, Value};

mod common;

/// What the model receives, checked: valid, in budget, and honest when cut.
fn as_the_model_reads_it(name: &str, v: Value) -> Value {
    let raw = v.to_string().len();
    let out = fit(v, MAX_RESULT_CHARS);
    let text = out.to_string();
    assert!(
        text.len() <= MAX_RESULT_CHARS,
        "{name}: {} characters",
        text.len()
    );
    serde_json::from_str::<Value>(&text).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"));
    if raw > MAX_RESULT_CHARS {
        assert!(
            out.get("omitted").is_some()
                || out.get("partial").is_some()
                || text.contains("[continues:"),
            "{name} was cut without saying so"
        );
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_director_read_fits_the_tool_loop_and_says_what_it_left_out() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 11, 240, json!({}))
            .await;
    let store = app.stores.dig(&dig.id).unwrap();
    let ids = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        10_000,
    )
    .unwrap();
    assert!(ids.len() >= 200, "a large mailbox: {}", ids.len());
    // One bulk change, the kind whose Activity row carried every message id and its restore data.
    app.runtime
        .invoke_as_user(
            "move_to_spoil",
            json!({ "digId": dig.id, "messageIds": ids[..150] }),
        )
        .await
        .unwrap();
    // A message with a body far past any limit.
    let mut long = repo::messages::get(&store, &dig.id, &ids[0])
        .unwrap()
        .unwrap();
    long.body_text = Some("A long newsletter paragraph. ".repeat(3_000));
    repo::messages::upsert(&store, &long).unwrap();

    let reads = AppReads(app.clone());
    let scope = ReadScope::default();
    let page = json!({ "page": { "cursor": null, "limit": 200 } });
    let d = &dig.id;
    let results: Vec<(&str, Value)> = vec![
        (
            "list_messages",
            reads.list_messages(&scope, page.clone()).await.unwrap(),
        ),
        (
            "search",
            reads
                .search(
                    &scope,
                    json!({ "query": "the", "page": { "cursor": null, "limit": 200 } }),
                )
                .await
                .unwrap(),
        ),
        (
            "list_finds",
            reads.list_finds(&scope, page.clone()).await.unwrap(),
        ),
        (
            "list_activity",
            reads.list_activity(&scope, page.clone()).await.unwrap(),
        ),
        (
            "list_meshes",
            reads.list_meshes(&scope, json!({})).await.unwrap(),
        ),
        (
            "list_signals",
            reads.list_signals(&scope, json!({})).await.unwrap(),
        ),
        (
            "get_message",
            reads.get_message(&scope, d, &ids[0]).await.unwrap(),
        ),
        (
            "get_thread",
            reads.get_thread(&scope, d, &long.thread_id).await.unwrap(),
        ),
        ("explain", reads.explain(&scope, d, &ids[1]).await.unwrap()),
    ];
    for (name, v) in results {
        as_the_model_reads_it(name, v);
    }

    // The leaner rows: a bulk change reads as a count, not 150 ids and their restore data.
    let acts = reads
        .list_activity(&scope, json!({ "action": "move_to_spoil" }))
        .await
        .unwrap();
    let row = &acts["items"][0];
    assert_eq!(row["affected"]["count"], 150);
    assert_eq!(row["affected"]["first"].as_array().unwrap().len(), 5);
    assert!(row["undo"].get("restore").is_none(), "{}", row["undo"]);
    // A long body keeps its start and says how much more there was.
    let msg = as_the_model_reads_it(
        "get_message",
        reads.get_message(&scope, d, &ids[0]).await.unwrap(),
    );
    assert!(msg["bodyText"].as_str().unwrap().contains("[continues:"));
    // The Signal index, and one Signal in full.
    let index = reads.list_signals(&scope, json!({})).await.unwrap();
    assert!(index[0].get("choices").is_none());
    let name = index[0]["name"].as_str().unwrap().to_string();
    let one = reads
        .list_signals(&scope, json!({ "name": name }))
        .await
        .unwrap();
    assert!(one[0].get("choices").is_some());
}
