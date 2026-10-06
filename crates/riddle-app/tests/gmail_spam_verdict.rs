//! Gmail's spam verdict can arrive after Riddle has decided a message. A
//! scam opener sat Unresolved for days after Gmail moved it to Spam: sync
//! stored the new label and nothing read it, because facts are computed when
//! a message is sifted. Now a change to `SPAM` from outside Riddle sends the
//! message back to Sift, in both directions.

use riddle_core::*;
use riddle_mail::MailProvider;
use riddle_store::repo;
use riddle_sync::engine::CancelFlag;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_later_gmail_spam_verdict_sends_the_message_back_to_sift_and_not_spam_undoes_it() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 11, 40, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    app.sift_pass(&dig, 500, None).await.unwrap();

    // A received message nobody protects, decided as something other than
    // Contamination.
    let target = repo::facts::ids(&store)
        .unwrap()
        .into_iter()
        .find(|id| {
            let f = repo::facts::get(&store, id).unwrap().unwrap();
            let k = repo::classifications::current(&store, id).unwrap().unwrap();
            !f.is_sent
                && !f.gmail_spam
                && !f.known()
                && !f.same_organisation
                && !f.sender_in_never_hidden
                && !f.sender_is_vip
                && k.disposition.kind() != DispositionKind::Contamination
                && k.mechanism != DecisionMechanism::User
        })
        .expect("the fixture has an unprotected received message");
    let provider = app
        .services
        .require::<riddle_mail::registry::ProviderRegistry>()
        .unwrap()
        .fixture(&dig.id)
        .unwrap();
    let pid = target.provider_id().to_string();
    let cancel = CancelFlag::new();
    let decided = |app: &riddle_app::App| {
        repo::classifications::current(&app.stores.dig(&dig.id).unwrap(), &target)
            .unwrap()
            .unwrap()
    };

    // Gmail moves it to Spam after the fact.
    provider
        .modify_labels(
            std::slice::from_ref(&pid),
            &["SPAM".into()],
            &["INBOX".into()],
        )
        .await
        .unwrap();
    app.sync_pass(&dig, &cancel, &mut |_| {}).await.unwrap();
    app.sift_pass(&dig, 500, None).await.unwrap();
    let k = decided(&app);
    assert_eq!(k.disposition.kind(), DispositionKind::Contamination);
    assert!(k.reason.contains("Gmail marked it as spam"), "{}", k.reason);

    // The user says "Not spam".
    provider
        .modify_labels(
            std::slice::from_ref(&pid),
            &["INBOX".into()],
            &["SPAM".into()],
        )
        .await
        .unwrap();
    app.sync_pass(&dig, &cancel, &mut |_| {}).await.unwrap();
    app.sift_pass(&dig, 500, None).await.unwrap();
    let k = decided(&app);
    assert!(
        !k.reason.contains("Gmail marked it as spam"),
        "Not spam lifts the verdict: {}",
        k.reason
    );
}
