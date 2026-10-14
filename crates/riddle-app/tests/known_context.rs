//! A reply in a thread the user is already in, or from a new person at a
//! firm they know, is not a stranger's mail. An accountant's reply in a
//! thread the user's household was in ended Unresolved, because the user
//! had never written to that address (13 Oct 2026).

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn message(
    dig: &DigId,
    pid: &str,
    thread: &str,
    from: &str,
    minutes_ago: i64,
    sent: bool,
) -> Message {
    Message {
        id: MessageId::for_provider(dig, pid),
        dig_id: dig.clone(),
        thread_id: ThreadId::for_provider(dig, thread),
        provider_id: pid.into(),
        subject: "Question about medical expenses and benefits through the corporation".into(),
        from: Address::new(from, None),
        to: vec![],
        cc: vec![],
        bcc: vec![],
        date: chrono::Utc::now() - chrono::Duration::minutes(minutes_ago),
        snippet: String::new(),
        labels: vec![],
        headers: MessageHeaders::default(),
        attachments: vec![],
        size_bytes: 0,
        is_sent: sent,
        has_body: true,
        body_text: Some("You can set up a health spending account through the corporation.".into()),
        body_html: None,
        synced_at: chrono::Utc::now(),
    }
}

fn person(email: &str, replied: bool) -> Person {
    Person {
        id: PersonId::new(),
        emails: vec![email.into()],
        name: None,
        organisation: None,
        relationship_strength: 0.5,
        first_contact_at: None,
        last_contact_at: None,
        replied_to_by_user: replied,
        messages_from: 3,
        messages_to: 1,
        automated: false,
        mailing_list: false,
        auth_failures: 0,
        spam_verdicts: 0,
        in_contacts: false,
        dig_ids: vec![],
        updated_at: chrono::Utc::now(),
        cadence: None,
        on_schedule: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reply_in_a_known_thread_or_from_a_known_firm_is_known() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 3, 1, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let index = app.stores.index();
    // The user replied to someone at the firm once; never to its CPA.
    repo::people::upsert(&index, &person("lauren@delyaccounting.example", true)).unwrap();
    repo::people::upsert(&index, &person("helper@acme.zendesk.com", true)).unwrap();
    let mine = message(&dig.id, "p1", "t1", "me@gmail.com", 60, true);
    let cpa = message(
        &dig.id,
        "p2",
        "t1",
        "terry@delyaccounting.example",
        30,
        false,
    );
    let new_thread = message(
        &dig.id,
        "p3",
        "t2",
        "terry@delyaccounting.example",
        20,
        false,
    );
    let stranger = message(&dig.id, "p4", "t3", "someone@elsewhere.example", 10, false);
    let help_desk = message(&dig.id, "p5", "t4", "other@acme.zendesk.com", 5, false);
    for m in [&mine, &cpa, &new_thread, &stranger, &help_desk] {
        repo::messages::upsert(&store, m).unwrap();
    }
    let known = |m: &Message| {
        let p = app.sift_input(&dig, m.clone()).unwrap().people;
        (p.known_thread, p.known_domain)
    };
    assert_eq!(
        known(&cpa),
        (true, true),
        "a reply after the user's own message, from a known firm"
    );
    assert_eq!(
        known(&new_thread),
        (false, true),
        "a new thread from the firm: known by its domain"
    );
    assert_eq!(known(&stranger), (false, false));
    assert_eq!(
        known(&help_desk),
        (false, false),
        "a shared help desk says nothing about the next sender"
    );
}
