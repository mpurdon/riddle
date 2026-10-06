//! The Priority inbox shows only what the user chose: a starred message, a
//! message carrying a chosen Stratum, and the source of an open Find at or
//! above the chosen priority, and not a message that is none of those. The
//! Finds live in the index store and the messages in the Dig store; this
//! proves the two are joined the way the settings say.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn find_for(dig: &DigId, m: &MessageSummary, priority: u8) -> Find {
    let now = chrono::Utc::now();
    Find {
        code: None,
        id: FindId::new(),
        find_type: FindType::Respond,
        title: "needs you".into(),
        summary: String::new(),
        priority,
        status: FindStatus::Open,
        due_at: None,
        snoozed_until: None,
        created_at: now,
        updated_at: now,
        resolved_at: None,
        resolved_by: None,
        sources: vec![FindSource {
            message_id: m.id.clone(),
            thread_id: m.thread_id.clone(),
            dig_id: dig.clone(),
        }],
        dig_ids: vec![dig.clone()],
        identity_key: format!("priority-test:{}", m.id),
        reason: "a test".into(),
        confidence: None,
        classification_id: None,
        trace_id: None,
        superseded_by: None,
        series: None,
        expires_at: None,
        kept_open_at: None,
        notify: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_priority_inbox_surfaces_starred_chosen_strata_and_high_priority_finds_only() {
    let (app, ops) = common::boot_scripted(false, |s| {
        s.priority_inbox.strata = vec!["Finance".into()];
        s.priority_inbox.labels = vec!["STARRED".into()];
        s.priority_inbox.min_find_priority = Some(70);
        s.priority_inbox.find_types = vec![];
        // Importance is its own criterion (tests/importance.rs).
        s.priority_inbox.min_importance = None;
    })
    .await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 5, 20, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    // Start from a known state: no fixture Find, no fixture star, no
    // Finance Stratum on the four messages under test.
    for mut f in repo::finds::list_by_status(&index, FindStatus::Open, None).unwrap() {
        f.status = FindStatus::Dismissed;
        repo::finds::upsert(&index, &f).unwrap();
    }
    let all = app
        .list_messages(
            Some(&dig.id),
            &repo::messages::MessageFilter::default(),
            &PageRequest::default(),
        )
        .unwrap();
    assert!(all.items.len() >= 4, "{}", all.items.len());
    for m in &all.items {
        repo::messages::set_labels(&store, &m.id, &["INBOX".to_string()]).unwrap();
    }
    let (starred, finance, found, plain) =
        (&all.items[0], &all.items[1], &all.items[2], &all.items[3]);
    repo::messages::set_labels(&store, &starred.id, &["INBOX".into(), "STARRED".into()]).unwrap();
    repo::messages::set_strata(&store, &finance.id, &["Finance".to_string()]).unwrap();
    repo::messages::set_strata(&store, &found.id, &[]).unwrap();
    repo::messages::set_strata(&store, &plain.id, &["Travel".to_string()]).unwrap();
    repo::finds::upsert(&index, &find_for(&dig.id, found, 90)).unwrap();
    // A Find below the threshold does not surface its message.
    repo::finds::upsert(&index, &find_for(&dig.id, plain, 30)).unwrap();

    let page = app
        .priority_inbox(Some(&dig.id), &PageRequest::default())
        .unwrap();
    let ids: Vec<&MessageId> = page.items.iter().map(|m| &m.id).collect();
    assert!(ids.contains(&&starred.id), "starred");
    assert!(ids.contains(&&finance.id), "chosen Stratum");
    assert!(ids.contains(&&found.id), "high-priority Find");
    assert!(!ids.contains(&&plain.id), "none of the criteria");
    assert_eq!(page.items.len(), 3, "{ids:?}");
    // Across all Digs the same rows come back, so the rail's All view agrees.
    let across = app.priority_inbox(None, &PageRequest::default()).unwrap();
    assert_eq!(across.items.len(), 3);

    // Lowering the threshold to "any" lets the low Find's message through;
    // limiting the types to one the Finds are not shuts both out.
    let settings = app
        .services
        .require::<riddle_settings::SettingsStore>()
        .unwrap();
    let mut s = settings.settings();
    s.priority_inbox.min_find_priority = None;
    settings.save(s.clone()).unwrap();
    let page = app
        .priority_inbox(Some(&dig.id), &PageRequest::default())
        .unwrap();
    assert_eq!(page.items.len(), 4);
    s.priority_inbox.find_types = vec![FindType::Pay];
    settings.save(s).unwrap();
    let page = app
        .priority_inbox(Some(&dig.id), &PageRequest::default())
        .unwrap();
    let ids: Vec<&MessageId> = page.items.iter().map(|m| &m.id).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert!(!ids.contains(&&found.id));
}
