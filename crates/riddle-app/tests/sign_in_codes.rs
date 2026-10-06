//! A sign-in code is worth something for minutes. The Accounts pack reads
//! the code out of the message (`verification_code`) and raises a Critical
//! Find that tells the user at once, carries the code to copy, and lapses
//! after 15 minutes; a newsletter about code, or a booking's confirmation
//! code, is not one. Before, the rule guessed from the subject and half the
//! code mail went to Security and account alerts as plain notices.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn mail(dig: &DigId, pid: &str, from: &str, subject: &str, body: &str) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new(from, None);
    m.to = vec![Address::new("me@gmail.com", None)];
    m.subject = subject.into();
    m.body_text = Some(body.into());
    m.has_body = true;
    m.headers.message_id = Some(format!("<{pid}@example>"));
    m.headers.authentication = AuthenticationResults {
        spf: AuthVerdict::Pass,
        dkim: AuthVerdict::Pass,
        dmarc: AuthVerdict::Pass,
    };
    m
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sign_in_code_is_a_critical_find_that_carries_the_code_and_lapses_in_minutes() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    let accounts = riddle_mesh::packs::builtin_packs()
        .into_iter()
        .find(|p| p.id == "accounts")
        .unwrap();
    common::install_mesh(
        &app,
        &accounts.meshes[0]
            .text
            .replacen("\nScope:", "\nPriority: -5\nScope:", 1),
        MeshStatus::Active,
    );
    let store = app.stores.dig(&dig.id).unwrap();
    for m in [
        mail(
            &dig.id,
            "code",
            "help@fb02.freshbooks.example",
            "Your FreshBooks code - 002534",
            "Here is your code This code will expire in 10 minutes. 002534",
        ),
        mail(
            &dig.id,
            "booking",
            "trips@airline.example",
            "Your booking is confirmed",
            "Your confirmation code is 482910. Departs 2026-10-04.",
        ),
    ] {
        repo::messages::upsert(&store, &m).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();

    let index = app.stores.index();
    let code = MessageId::for_provider(&dig.id, "code");
    let k = repo::classifications::current(&store, &code)
        .unwrap()
        .unwrap();
    assert_eq!(
        (k.disposition.clone(), k.importance),
        (
            Disposition::Find {
                find_type: FindType::Know
            },
            Importance::Critical
        ),
        "{}",
        k.reason
    );
    let find = repo::finds::for_message(&index, &code)
        .unwrap()
        .pop()
        .expect("the code raised a Find");
    assert_eq!(find.code.as_deref(), Some("002534"));
    assert!(find.notify, "it tells the user at once");
    let message = repo::messages::get(&store, &dig.id, &code)
        .unwrap()
        .unwrap();
    assert_eq!(
        find.expires_at,
        Some(message.date + chrono::Duration::minutes(15))
    );

    let booking = MessageId::for_provider(&dig.id, "booking");
    assert!(repo::finds::for_message(&index, &booking)
        .unwrap()
        .iter()
        .all(|f| f.code.is_none()));
    let b = repo::classifications::current(&store, &booking)
        .unwrap()
        .unwrap();
    assert_ne!(b.importance, Importance::Critical, "{}", b.reason);
}
