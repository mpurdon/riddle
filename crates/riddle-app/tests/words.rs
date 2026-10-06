//! Decisions made before reasons were written in words get theirs at boot,
//! from the stored trace, without a judge call: the classification and the
//! Find that carries it both read as the sentence afterwards. Once per Dig.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn old_reasons_are_rewritten_in_words_once_per_dig() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 30, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    // A decision written the old way: "Mesh vN: <rule as written>".
    let mut old = repo::classifications::current_mesh_decisions(&store)
        .unwrap()
        .into_iter()
        .find(|k| {
            k.mechanism == DecisionMechanism::Mesh && k.disposition != Disposition::Unresolved
        })
        .expect("the fixture mailbox has a Mesh decision");
    let grammar = format!(
        "Some Mesh v{}: {}",
        old.mesh_version.unwrap(),
        old.matched_rule.clone().unwrap()
    );
    let write_grammar = |k: &mut Classification| {
        k.reason = grammar.clone();
        repo::classifications::update(&store, k).unwrap();
        for mut f in repo::finds::for_message(&index, &k.message_id).unwrap() {
            f.reason = grammar.clone();
            repo::finds::upsert(&index, &f).unwrap();
        }
    };
    write_grammar(&mut old);
    let finds = repo::finds::for_message(&index, &old.message_id).unwrap();

    let changed = app.rewrite_reasons_in_words(&dig.id).unwrap();
    assert!(changed >= 1, "{changed}");
    let now = repo::classifications::current(&store, &old.message_id)
        .unwrap()
        .unwrap();
    assert_eq!(now.id, old.id, "rewritten in place, not superseded");
    assert!(
        now.reason.starts_with("It appears ") || now.reason.starts_with("Nothing above applied"),
        "{}",
        now.reason
    );
    assert!(now.reason.contains(", so "), "{}", now.reason);
    for f in finds {
        let f = repo::finds::get(&index, &f.id).unwrap().unwrap();
        if f.classification_id.as_ref() == Some(&old.id) {
            assert_eq!(f.reason, now.reason, "the Find carries the same sentence");
        }
    }
    // A second pass has nothing left to change.
    assert_eq!(app.rewrite_reasons_in_words(&dig.id).unwrap(), 0);

    // Once per Dig: the boot-time pass sets the flag and does not run again,
    // even when a reason has gone back to the grammar.
    let flag = format!("reasons_in_words:{}", dig.id);
    repo::kv::set(&index, &flag, "0").unwrap();
    app.rewrite_all_reasons_in_words();
    assert_eq!(repo::kv::get(&index, &flag).unwrap().as_deref(), Some("1"));
    let mut again = repo::classifications::current(&store, &old.message_id)
        .unwrap()
        .unwrap();
    write_grammar(&mut again);
    app.rewrite_all_reasons_in_words();
    assert_eq!(
        repo::classifications::current(&store, &old.message_id)
            .unwrap()
            .unwrap()
            .reason,
        grammar,
        "the flag is set, so the pass leaves the Dig alone"
    );
}
