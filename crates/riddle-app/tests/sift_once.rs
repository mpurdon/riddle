//! A message is sifted once, however many jobs ask at the same time. A
//! sync, a "decide again" and a catch-up each run a Sift pass; side by side
//! they read the same unclassified ids, and after a two-week gap 455 of 924
//! messages were judged twice, paying for both.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_sift_passes_at_once_judge_each_message_once() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let ids: Vec<MessageId> = (0..12)
        .map(|i| {
            let mut m = riddle_mail::fixture::blank_message(dig.id.as_str(), &format!("new{i}"));
            m.from = Address::new(format!("sender{i}@shop.example"), None);
            m.subject = format!("Order {i} has shipped");
            m.body_text = Some("Your order is on its way.".into());
            m.has_body = true;
            repo::messages::upsert(&store, &m).unwrap();
            m.id
        })
        .collect();

    // Separate tasks on separate threads, as the job runner runs them.
    let pass = || {
        let (app, dig) = (app.clone(), dig.clone());
        tokio::spawn(async move { app.sift_pass(&dig, 100, None).await.unwrap() })
    };
    let (a, b) = (pass(), pass());
    let (a, b) = (a.await.unwrap(), b.await.unwrap());
    assert_eq!(a.processed + b.processed, ids.len() as u64, "{a:?} {b:?}");
    for id in &ids {
        assert_eq!(
            repo::classifications::history(&store, id).unwrap().len(),
            1,
            "{id}"
        );
    }
}
