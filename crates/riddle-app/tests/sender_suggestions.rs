//! A sender with enough mail and on no list is judged once and offered the
//! list it belongs on; once it is on the list the offer goes. The scripted
//! judge picks the first choice (a cloud or hosting provider) at .6.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

const SENDER: &str = "billing@acme-cloud.example";

fn bill(dig: &DigId, pid: &str, hours_ago: i64) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.date = chrono::SubsecRound::trunc_subsecs(chrono::Utc::now(), 3)
        - chrono::Duration::hours(hours_ago);
    m.subject = format!("Your Acme Cloud statement {pid} is ready");
    m.snippet = "Your monthly statement for compute and storage is ready to view.".into();
    m.from = Address::new(SENDER, Some("Acme Cloud"));
    m.to = vec![Address::new("me@example.com", None)];
    m.headers.message_id = Some(format!("<{pid}@acme-cloud.example>"));
    m.headers.authentication.dkim = AuthVerdict::Pass;
    m
}

/// A GitHub notification: the GitHub pack's `[detect]` names github.com.
fn notification(dig: &DigId, pid: &str, hours_ago: i64) -> Message {
    let mut m = bill(dig, pid, hours_ago);
    m.subject = format!("[acme/widgets] Build {pid} failed");
    m.snippet = "The nightly build failed on main.".into();
    m.from = Address::new("notifications@github.com", Some("GitHub"));
    m.headers.message_id = Some(format!("<{pid}@github.com>"));
    m
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_vendor_is_judged_once_and_offered_its_list_until_it_is_on_it() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@example.com", 5, 5, json!({})).await;
    // The Engineering pack brings the engineering_vendors list; the GitHub
    // pack recognises github.com.
    for pack in ["engineering", "developer-github"] {
        app.runtime
            .invoke_as_user(
                "install_pack",
                json!({ "packId": pack, "status": "active", "replace": true }),
            )
            .await
            .unwrap();
    }
    let store = app.stores.dig(&dig.id).unwrap();
    for (pid, h) in [("s1", 72), ("s2", 48), ("s3", 24)] {
        repo::messages::upsert(&store, &bill(&dig.id, pid, h)).unwrap();
    }
    // GitHub writes as often, and the GitHub pack recognises it: its mail
    // is the pack's business, so it is neither judged nor offered.
    for (pid, h) in [("g1", 70), ("g2", 46), ("g3", 22)] {
        repo::messages::upsert(&store, &notification(&dig.id, pid, h)).unwrap();
    }

    // Nothing judged yet: no offer. One review pass judges the one sender.
    let before = app.sender_suggestions(&[]).unwrap();
    assert!(
        before.iter().all(|s| s.entry != "acme-cloud.example"),
        "{before:?}"
    );
    assert_eq!(app.review_senders(&dig, 8).await.unwrap(), 1);
    // A second pass finds nothing new to judge: one judgment per domain.
    assert_eq!(app.review_senders(&dig, 8).await.unwrap(), 0);

    let offers = app.sender_suggestions(&[]).unwrap();
    let ours = offers
        .iter()
        .find(|s| s.entry == "acme-cloud.example")
        .expect("an offer for the new vendor");
    assert_eq!(ours.list, "engineering_vendors");
    assert_eq!(ours.kind, Some(SenderKind::CloudOrHosting));
    assert_eq!(ours.messages, 3);
    assert_eq!(ours.sender, SENDER);
    assert!(ours.text.contains("engineering_vendors"), "{}", ours.text);
    assert!(
        offers.iter().all(|s| !s.entry.ends_with("github.com")),
        "{offers:?}"
    );

    // On the list, the offer goes; the judgment stays cached.
    app.runtime
        .invoke_as_user(
            "save_list",
            json!({ "name": "engineering_vendors", "entries": ["acme-cloud.example"] }),
        )
        .await
        .unwrap();
    let after = app.sender_suggestions(&[]).unwrap();
    assert!(
        after.iter().all(|s| s.entry != "acme-cloud.example"),
        "{after:?}"
    );
    assert_eq!(app.review_senders(&dig, 8).await.unwrap(), 0);
}
