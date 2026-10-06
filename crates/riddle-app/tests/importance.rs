//! Importance: every message is Low, Normal, High or Critical. A route sets
//! it (`+ Importance: critical`), Sift derives it otherwise (hidden mail is
//! Low), and the user's own level for a message outlives the rule deciding
//! it again. The Find a message raised moves into its level's priority band,
//! and lists filter on the level.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

const RULE: &str = "Mesh: Codes\nPriority: 0\nROUTES\nIF subject CONTAINS \"sign-in code\" → Find: Know + Importance: Critical\nELSE IF subject CONTAINS \"weekly deals\" → Spoil";

fn mail(dig: &DigId, pid: &str, subject: &str) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new("noreply@shop.example", None);
    m.to = vec![Address::new("me@gmail.com", None)];
    m.subject = subject.into();
    m.body_text = Some("Hello.".into());
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
async fn a_rule_sets_importance_the_user_overrides_it_and_a_redecision_keeps_theirs() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    common::install_mesh(&app, RULE, MeshStatus::Active);
    let store = app.stores.dig(&dig.id).unwrap();
    for m in [
        mail(&dig.id, "code", "Your sign-in code"),
        mail(&dig.id, "deals", "Our weekly deals"),
    ] {
        repo::messages::upsert(&store, &m).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();
    let code = MessageId::for_provider(&dig.id, "code");
    let deals = MessageId::for_provider(&dig.id, "deals");
    let current = |id: &MessageId| repo::classifications::current(&store, id).unwrap().unwrap();
    assert_eq!(current(&code).importance, Importance::Critical);
    assert_eq!(current(&deals).importance, Importance::Low, "Spoil is Low");
    let index = app.stores.index();
    let find = || {
        repo::finds::for_message(&index, &code)
            .unwrap()
            .pop()
            .expect("the code raised a Find")
    };
    assert!(find().priority >= 90, "{}", find().priority);

    // Lists filter on the level.
    let page = PageRequest::default();
    let critical = repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter {
            min_importance: Some(Importance::Critical),
            ..Default::default()
        },
        &page,
    )
    .unwrap();
    assert!(critical.items.iter().any(|m| m.id == code));
    assert!(critical
        .items
        .iter()
        .all(|m| m.importance == Importance::Critical && m.id != deals));

    // The user says it is only Normal: the Find moves into Normal's band.
    let out = app
        .runtime
        .invoke_as_user(
            "set_importance",
            json!({ "digId": dig.id, "messageId": code, "importance": "normal" }),
        )
        .await
        .unwrap();
    assert!((35..70).contains(&find().priority), "{}", find().priority);

    // The rule decides the message again; the user's level stands.
    repo::classifications::supersede_current(&store, &code).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(current(&code).importance, Importance::Critical);
    let listed = |id: &MessageId| {
        repo::messages::list(&store, &dig.id, &Default::default(), &page)
            .unwrap()
            .items
            .into_iter()
            .find(|m| &m.id == id)
            .unwrap()
    };
    let row = listed(&code);
    assert_eq!(
        (row.importance, row.importance_set_by_you),
        (Importance::Normal, true)
    );

    // Undo gives the rule's level back.
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": out.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    let row = listed(&code);
    assert_eq!(
        (row.importance, row.importance_set_by_you),
        (Importance::Critical, false)
    );
}
