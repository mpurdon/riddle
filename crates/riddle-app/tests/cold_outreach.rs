//! Spam Gmail had not flagged yet: a scam opener that impersonated an
//! investor from a numbered Gmail address ("I noticed your comment on my
//! YouTube channel… | look forward to your response") passed SPF, DKIM and
//! DMARC and sat Unresolved until Gmail moved it to spam days later. Riddle
//! can see the same thing without Gmail: one-to-one mail from a stranger's
//! free mailbox (`cold_outreach`) that invents a shared past
//! (`claims_prior_contact`) or disguises its letters (`glyph_substitution`).
//! The same pass files the user's own drafts, which were being read as mail
//! from a first-time sender, and catches mail forged from the user's own
//! address.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

const RULE: &str = "Mesh: Cold outreach\nPriority: 10\nROUTES\nWHEN cold_outreach\n    IF glyph_substitution → Contamination\n    ELSE IF claims_prior_contact@v1 > .6 → Contamination\nEND";

fn mail(dig: &DigId, pid: &str, from: &str, body: &str) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new(from, None);
    m.to = vec![Address::new("me@gmail.com", None)];
    m.subject = "Reaching out".into();
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
async fn cold_outreach_that_invents_a_shared_past_is_contamination_without_gmails_verdict() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    common::install_mesh(&app, RULE, MeshStatus::Active);
    let store = app.stores.dig(&dig.id).unwrap();

    let invented = mail(
        &dig.id,
        "invented",
        "investor96@gmail.com",
        "I recently noticed your comment on my channel and wanted to connect.",
    );
    let disguised = mail(
        &dig.id,
        "disguised",
        "recruiter.jobs@gmail.com",
        "A discreet opportunity. Thank you, and | look forward to your response.",
    );
    let introduced = mail(
        &dig.id,
        "introduced",
        "neighbour@gmail.com",
        "Hello, I live two doors down and I am organising the street party. Could you let me know if Saturday works? Thanks.",
    );
    // A shop answering the user's website form: a free mailbox, no thread
    // in the mailbox, and "saw your message" is true. It is a reply.
    let mut form_reply = mail(
        &dig.id,
        "form_reply",
        "shop@gmail.com",
        "Thanks for the email. We saw your message and would be happy to help.",
    );
    form_reply.headers.in_reply_to = Some("<form-123@shop.example>".into());
    let mut draft = mail(&dig.id, "draft", "me@gmail.com", "Notes to self");
    draft.labels = vec!["DRAFT".into()];
    draft.headers.authentication = AuthenticationResults::default();
    let mut forged = mail(
        &dig.id,
        "forged",
        "me@gmail.com",
        "I have access to your account.",
    );
    forged.headers.authentication.dmarc = AuthVerdict::Fail;
    for m in [
        &invented,
        &disguised,
        &introduced,
        &form_reply,
        &draft,
        &forged,
    ] {
        repo::messages::upsert(&store, m).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();

    let current = |pid: &str| {
        repo::classifications::current(&store, &MessageId::for_provider(&dig.id, pid))
            .unwrap()
            .unwrap()
    };
    let i = current("invented");
    assert_eq!(
        i.disposition.kind(),
        DispositionKind::Contamination,
        "{}",
        i.reason
    );
    let g = current("disguised");
    assert_eq!(
        g.disposition.kind(),
        DispositionKind::Contamination,
        "{}",
        g.reason
    );
    assert_ne!(
        current("introduced").disposition.kind(),
        DispositionKind::Contamination,
        "a stranger who introduces themselves is not a scam"
    );
    assert!(
        !matches!(
            current("form_reply").disposition.kind(),
            DispositionKind::Contamination | DispositionKind::Spoil
        ),
        "a reply to the user's own enquiry is never hidden as cold outreach"
    );
    let d = current("draft");
    assert_eq!(d.disposition.kind(), DispositionKind::Filed);
    assert_eq!(d.matched_rule.as_deref(), Some("drafts are Filed"));
    let f = current("forged");
    assert_eq!(
        f.disposition.kind(),
        DispositionKind::Contamination,
        "{}",
        f.reason
    );
    assert!(f.reason.contains("your own address"), "{}", f.reason);
}
