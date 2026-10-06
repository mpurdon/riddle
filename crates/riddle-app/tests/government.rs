//! Official government mail is the most urgent thing Riddle raises: a rule
//! on `government_sender` makes it a Do Find that interrupts at once and
//! sorts first. The fact is deterministic and asks for DMARC, because the
//! Canada Revenue Agency and the RCMP are what scams most often pretend to
//! be: their name on a forged From line, or a domain that only ends in
//! "canada.ca", must not become the most urgent thing on the list.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

const RULE: &str = "Mesh: Government\nPriority: -50\nROUTES\nIF government_sender → Find: Do + Notify + Stratum: Government\nGUARDS\nNEVER Spoil IF government_sender";

fn mail(dig: &DigId, pid: &str, from: &str, dmarc: AuthVerdict) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new(from, None);
    m.to = vec![Address::new("me@example.com", None)];
    m.subject = "Notice of assessment".into();
    m.headers.message_id = Some(format!("<{pid}@example>"));
    m.headers.authentication.dmarc = dmarc;
    m
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticated_government_mail_is_an_urgent_find_and_lookalikes_are_not() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@example.com", 5, 5, json!({})).await;
    common::install_mesh(&app, RULE, MeshStatus::Active);
    let store = app.stores.dig(&dig.id).unwrap();
    for m in [
        mail(
            &dig.id,
            "cra",
            "notifications@cra-arc.gc.ca",
            AuthVerdict::Pass,
        ),
        mail(
            &dig.id,
            "forged",
            "notifications@cra-arc.gc.ca",
            AuthVerdict::Fail,
        ),
        mail(
            &dig.id,
            "gas",
            "offers@email.petro-canada.ca",
            AuthVerdict::Pass,
        ),
    ] {
        repo::messages::upsert(&store, &m).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();

    let decided_by_rule = |pid: &str| {
        repo::classifications::current(&store, &MessageId::for_provider(&dig.id, pid))
            .unwrap()
            .unwrap()
            .matched_rule
            .is_some_and(|r| r.contains("government_sender"))
    };
    assert!(decided_by_rule("cra"));
    assert!(
        !decided_by_rule("forged"),
        "a forged From line is not the CRA"
    );
    assert!(!decided_by_rule("gas"), "petro-canada.ca is not canada.ca");

    let find = repo::finds::for_message(
        &app.stores.index(),
        &MessageId::for_provider(&dig.id, "cra"),
    )
    .unwrap()
    .remove(0);
    assert_eq!(find.find_type, FindType::Do);
    assert!(find.notify, "it interrupts at once");
    assert!(find.priority >= 95, "it sorts first: {}", find.priority);
}
