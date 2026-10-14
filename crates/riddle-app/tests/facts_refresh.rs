//! Facts stored before a field existed gain it at boot from the message, so
//! a rule on it simulates truthfully against history: `github_reason` from
//! the footer, `subject` and `headers` from the message itself. Once per Dig.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn github_message(dig: &DigId, pid: &str, body: &str) -> Message {
    Message {
        id: MessageId::for_provider(dig, pid),
        dig_id: dig.clone(),
        thread_id: ThreadId::for_provider(dig, &format!("t-{pid}")),
        provider_id: pid.into(),
        subject: "[acme/sync] Review requested (PR #1)".into(),
        from: Address::new("notifications@github.com", None),
        to: vec![Address::new("me@example.com", None)],
        cc: vec![],
        bcc: vec![],
        date: chrono::Utc::now(),
        snippet: body.chars().take(80).collect(),
        labels: vec!["INBOX".into()],
        headers: MessageHeaders {
            message_id: Some(format!("<{pid}@example>")),
            extra: [("x-acme-team".to_string(), "sync".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        },
        attachments: vec![],
        size_bytes: 0,
        is_sent: false,
        has_body: true,
        body_text: Some(body.into()),
        body_html: None,
        synced_at: chrono::Utc::now(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stored_facts_of_older_mail_gain_the_message_fields_once_per_dig() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 5, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    let old = github_message(
        &dig.id,
        "gh-old",
        "Please review.\n\nYou are receiving this because your review was requested.\n",
    );
    let unknown = github_message(&dig.id, "gh-odd", "No footer at all.");
    // Facts as an earlier build stored them: the domain and nothing else.
    for m in [&old, &unknown] {
        repo::messages::upsert(&store, m).unwrap();
        repo::facts::put(
            &store,
            &m.id,
            &Facts {
                sender_domain: "github.com".into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    // Both rows gain subject, sender and headers; only the one with a footer
    // gains a reason.
    let changed = app.refresh_message_facts(&dig.id).unwrap();
    assert!(changed >= 2, "{changed}");
    let refreshed = repo::facts::get(&store, &old.id).unwrap().unwrap();
    assert_eq!(refreshed.github_reason.as_deref(), Some("review_requested"));
    assert_eq!(refreshed.subject, old.subject);
    assert_eq!(refreshed.sender, "notifications@github.com");
    assert_eq!(
        refreshed.headers.get("x-acme-team").map(String::as_str),
        Some("sync"),
        "a rule on `header X-Acme-Team` now has something to read"
    );
    assert_eq!(
        repo::facts::get(&store, &unknown.id)
            .unwrap()
            .unwrap()
            .github_reason,
        None,
        "a footer that says nothing leaves nothing"
    );
    // A second pass over the same rows changes nothing.
    assert_eq!(app.refresh_message_facts(&dig.id).unwrap(), 0);

    // Once per Dig: the boot-time pass sets the flag and does not run again,
    // even when a row has drifted.
    let flag = format!("message_facts_v14:{}", dig.id);
    repo::kv::set(&index, &flag, "0").unwrap();
    app.refresh_all_message_facts();
    assert_eq!(repo::kv::get(&index, &flag).unwrap().as_deref(), Some("1"));
    let mut drifted = repo::facts::get(&store, &old.id).unwrap().unwrap();
    drifted.github_reason = None;
    repo::facts::put(&store, &old.id, &drifted).unwrap();
    app.refresh_all_message_facts();
    assert_eq!(
        repo::facts::get(&store, &old.id)
            .unwrap()
            .unwrap()
            .github_reason,
        None,
        "the flag is set, so the pass leaves the Dig alone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn english_sale_mail_stored_as_portuguese_is_english_after_the_refresh() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 5, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let mut sale = github_message(&dig.id, "sale", "");
    sale.from = Address::new("sales@store.example", None);
    sale.subject = "SURPLUS SUNDAY Has Arrived!".into();
    sale.body_text = Some(
        "TM22 $499 https://store.example/tm22 Henry $1,249 https://store.example/henry ".repeat(20),
    );
    repo::messages::upsert(&store, &sale).unwrap();
    // As the old detector stored it.
    repo::facts::put(
        &store,
        &sale.id,
        &Facts {
            language: "pt".into(),
            ..Default::default()
        },
    )
    .unwrap();
    app.refresh_message_facts(&dig.id).unwrap();
    assert_eq!(
        repo::facts::get(&store, &sale.id)
            .unwrap()
            .unwrap()
            .language,
        "en"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stored_facts_gain_the_sender_s_rhythm_from_people() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 5, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    let start = chrono::Utc::now() - chrono::Duration::days(20);
    for d in 0..20 {
        let mut m = github_message(&dig.id, &format!("r{d}"), "Time to write.");
        m.from = Address::new("reminders@jots.example", None);
        m.date = start + chrono::Duration::days(d) + chrono::Duration::minutes(d % 3);
        repo::messages::upsert(&store, &m).unwrap();
        repo::facts::put(&store, &m.id, &Facts::default()).unwrap();
    }
    let profile = repo::user_profile::get(&index).unwrap();
    riddle_people::refresh_address(
        &index,
        &app.stores.open_digs(),
        "reminders@jots.example",
        &profile,
        chrono::Utc::now(),
    )
    .unwrap();
    app.refresh_message_facts(&dig.id).unwrap();
    let facts = repo::facts::get(&store, &MessageId::for_provider(&dig.id, "r3"))
        .unwrap()
        .unwrap();
    assert_eq!(facts.sender_cadence, "daily");
    assert!(facts.sender_on_schedule);
}
