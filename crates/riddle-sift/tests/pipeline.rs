//! End-to-end Sift scenarios over in-memory stores and the scripted judge.
//! Each test states the claim from the architecture it proves.

use async_trait::async_trait;
use riddle_actions::{Action, ActionContext, Services};
use riddle_core::*;
use riddle_judge::{NullJudge, ScriptedJudge};
use riddle_sift::{bootstrap, explain, DigContext, PeopleFacts, Sift, SiftInput};
use riddle_store::{repo, Stores};
use std::sync::Arc;

fn profile() -> UserProfile {
    UserProfile {
        addresses: vec!["me@work.example".into()],
        aliases: vec![],
        group_addresses: vec!["team@work.example".into()],
        name: Some("Sam".into()),
        role: Some("engineer".into()),
        organisation_domains: vec!["work.example".into()],
        active_projects: vec!["sync worker".into()],
        vips: vec!["boss@work.example".into()],
        family: vec![],
        never_hidden: vec!["weekly@rustnews.example".into()],
        timezone: None,
        contacts: vec![],
    }
}

fn message(pid: &str, from: &str, subject: &str, body: &str) -> Message {
    let dig = DigId("d1".into());
    Message {
        id: MessageId::for_provider(&dig, pid),
        dig_id: dig.clone(),
        thread_id: ThreadId::for_provider(&dig, &format!("t-{pid}")),
        provider_id: pid.into(),
        subject: subject.into(),
        from: Address::new(from, None),
        to: vec![Address::new("me@work.example", None)],
        cc: vec![],
        bcc: vec![],
        date: chrono::Utc::now(),
        snippet: body.chars().take(80).collect(),
        labels: vec!["INBOX".into()],
        headers: MessageHeaders {
            message_id: Some(format!("<{pid}@example>")),
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

fn harness(judge: Option<Arc<dyn SemanticJudge>>) -> (Sift, Arc<Stores>, Arc<RecordingSink>) {
    let stores = Arc::new(Stores::in_memory().unwrap());
    bootstrap(&stores).unwrap();
    let events = Arc::new(RecordingSink::default());
    let mut sift = Sift::new(
        stores.clone(),
        Arc::new(SystemClock),
        events.clone(),
        BuildStamp::from_env_vars(
            "0.1.0-test",
            Some("abc123"),
            Some("false"),
            Some("2026-09-22"),
            "test",
        ),
    );
    sift.judge = judge;
    (sift, stores, events)
}

fn input(m: Message, people: PeopleFacts) -> SiftInput {
    SiftInput::simple(m, profile(), people, DigContext::fixture("d1"))
}

fn github_review_request() -> Message {
    let mut m = message("gh1", "notifications@github.com", "[acme/sync] Review requested: Add retry to sync worker (PR #482)", "@me requested your review on: acme/sync#482 Add retry to the sync worker.\n\nYou are receiving this because your review was requested.\nReply to this email directly, view it on GitHub, or unsubscribe.");
    m.headers.list_id = Some("<acme/sync.acme.github.com>".into());
    m.headers.list_unsubscribe = Some("<mailto:unsub@github.com>".into());
    m.headers.authentication = AuthenticationResults {
        spf: AuthVerdict::Pass,
        dkim: AuthVerdict::Pass,
        dmarc: AuthVerdict::Pass,
    };
    m
}

/// A GitHub notification that says why it was sent. Older mail carries the
/// reason only in the footer, which is what this fixture has.
fn github_comment() -> Message {
    let template = github_review_request();
    let mut m = message(
        "gh2",
        "notifications@github.com",
        "Re: [acme/sync] Add retry to sync worker (PR #482)",
        "@someone left a comment on: acme/sync#482 Add retry to the sync worker.\n\nYou are receiving this because you commented.\nReply to this email directly, view it on GitHub, or unsubscribe.",
    );
    m.headers = MessageHeaders {
        message_id: Some("<gh2@example>".into()),
        ..template.headers
    };
    m
}

/// §2: the doc's GitHub example becomes Find: Review through the default
/// GitHub Mesh. The review request says so in its header or footer, so the
/// decision is a fact's and asks the judge nothing; a comment on a thread
/// asks the judge whether a reply is needed, and that trace is fully
/// populated.
#[tokio::test]
async fn the_github_review_request_becomes_a_review_find_with_a_full_trace() {
    let judge = Arc::new(ScriptedJudge::new());
    let (sift, stores, events) = harness(Some(judge.clone()));
    let mut outcome = sift
        .run(input(
            github_review_request(),
            PeopleFacts {
                mailing_list: true,
                ..Default::default()
            },
        ))
        .await
        .unwrap();
    assert_eq!(
        outcome.classification.disposition,
        Disposition::Find {
            find_type: FindType::Review
        },
        "{}",
        outcome.classification.reason
    );
    assert_eq!(
        outcome.classification.mechanism,
        DecisionMechanism::Deterministic,
        "a header states it; no Signal was asked"
    );
    assert_eq!(
        outcome.trace.mesh_name.as_deref(),
        Some("GitHub Notifications")
    );
    assert!(outcome
        .classification
        .matched_rule
        .as_deref()
        .unwrap()
        .contains("github_reason = review_requested"));
    // The reason is the route in words, read against this message.
    assert_eq!(
        outcome.classification.reason,
        "It appears the sender's domain is github.com and the GitHub notification reason is review_requested, so it is something to review under Engineering."
    );
    assert!(outcome
        .classification
        .strata
        .contains(&"Engineering".to_string()));
    // (Signal Selection still asks the judge for the Meshes in scope before
    // routing; the decision itself used none of the answers.)
    assert!(outcome.trace.build_stamp.starts_with("abc123"));
    assert_eq!(outcome.trace.app_version, "0.1.0-test");
    let find = outcome.find.clone().expect("a Find proposal");
    assert_eq!(find.find_type, FindType::Review);
    assert_eq!(find.identity_key, "<gh1@example>");
    assert!(
        find.priority >= 60,
        "directly addressed raises priority: {}",
        find.priority
    );

    // A comment on a thread you are in asks the judge, and the trace says so.
    let commented = sift
        .run(input(github_comment(), PeopleFacts::default()))
        .await
        .unwrap();
    assert!(!commented.trace.signals.is_empty());
    assert!(commented.trace.signals.iter().all(|s| !s.cached));
    assert_eq!(
        commented.trace.judge_identity.as_deref(),
        Some("scripted:scripted-1")
    );
    assert!(commented.trace.cost.judge_questions > 0);
    assert!(commented.trace.cost.judge_cost_usd.is_some());
    assert!(commented.note.contains("needed a fresh evaluation"));
    assert!(
        !matches!(
            commented.classification.disposition,
            Disposition::Spoil | Disposition::Contamination
        ),
        "{:?}: {}",
        commented.classification.disposition,
        commented.classification.reason
    );

    // Persist, then the "Why?" answer comes from the stored trace.
    let proposal = sift.persist(&mut outcome).unwrap();
    assert!(proposal.is_some());
    let store = stores.dig(&DigId("d1".into())).unwrap();
    let why = explain(&store, &outcome.classification.message_id).unwrap();
    assert_eq!(why.mesh_name.as_deref(), Some("GitHub Notifications"));
    assert!(why
        .matched
        .as_deref()
        .is_some_and(|m| m.contains("github_reason = review_requested")));
    assert!(repo::facts::get(&store, &outcome.classification.message_id)
        .unwrap()
        .is_some());
    let kinds: Vec<String> = events
        .take()
        .iter()
        .map(|e| format!("{e:?}").split(' ').next().unwrap_or("").to_string())
        .collect();
    assert!(kinds.iter().any(|k| k.starts_with("MessageClassified")));
    assert!(kinds.iter().any(|k| k.starts_with("PipelineStage")));

    // A second run of the same message is served from the cache.
    let again = sift
        .run(input(
            github_review_request(),
            PeopleFacts {
                mailing_list: true,
                ..Default::default()
            },
        ))
        .await
        .unwrap();
    assert_eq!(again.fresh, 0);
    assert!(again.cached > 0);
    assert!(again.note.contains("matched on cached Signals"));
}

/// README "Never Hidden": a never-hidden sender that a Mesh would Spoil
/// ends up not hidden, with the gate recorded.
#[tokio::test]
async fn a_never_hidden_sender_that_a_mesh_would_spoil_is_not_hidden_and_the_gate_is_recorded() {
    let (sift, _, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    let mut m = message(
        "nl1",
        "weekly@rustnews.example",
        "This Week in Rust 612",
        "This week in Rust: a newsletter digest. Unsubscribe at any time.",
    );
    m.headers.list_unsubscribe = Some("<https://rustnews.example/unsub>".into());
    m.headers.list_id = Some("<weekly.rustnews.example>".into());
    let outcome = sift
        .run(input(m.clone(), PeopleFacts::default()))
        .await
        .unwrap();
    assert!(
        !outcome.classification.disposition.hides(),
        "{:?}",
        outcome.classification.disposition
    );
    assert_eq!(outcome.classification.disposition, Disposition::Unresolved);
    let gate = outcome.trace.gate.clone().expect("gate recorded");
    assert!(gate.starts_with("never_hidden"), "{gate}");
    assert!(
        !outcome.trace.guard_vetoes.is_empty(),
        "the System guards Mesh vetoed the Spoil route"
    );

    // The same message from an ordinary list sender is Spoil.
    let mut plain = m;
    plain.from = Address::new("weekly@othernews.example", None);
    let outcome = sift
        .run(input(plain, PeopleFacts::default()))
        .await
        .unwrap();
    assert_eq!(
        outcome.classification.disposition,
        Disposition::Spoil,
        "{}",
        outcome.classification.reason
    );
    assert!(outcome
        .classification
        .strata
        .contains(&"Newsletters".to_string()));
}

/// §2, §58: a phishing lookalike is Contamination by deterministic evidence
/// and the judge is never called.
#[tokio::test]
async fn a_phishing_lookalike_becomes_contamination_without_any_judge_call() {
    let judge = Arc::new(ScriptedJudge::new());
    let (sift, _, _) = harness(Some(judge.clone()));
    let m = message("ph1", "security@paypa1.com", "Urgent: verify your account immediately", "Your account will be suspended. Click here to verify your account and confirm your password within 24 hours.");
    let outcome = sift.run(input(m, PeopleFacts::first_time())).await.unwrap();
    assert_eq!(
        outcome.classification.disposition,
        Disposition::Contamination
    );
    assert_eq!(outcome.classification.mechanism, DecisionMechanism::Gate);
    assert!(outcome
        .trace
        .gate
        .as_deref()
        .unwrap()
        .starts_with("contamination"));
    assert!(outcome.trace.signals.is_empty());
    assert_eq!(judge.calls(), 0, "Contamination never reaches the judge");
    assert!(outcome.trace.facts.lookalike_domain);
    assert!(outcome.trace.facts.urgency_language_present);
    assert!(outcome.find.is_none());
}

/// §18, invariant 37: with no judge, deterministic routes file the obvious,
/// everything else is Unresolved, and nothing is hidden.
#[tokio::test]
async fn judge_unavailable_means_unresolved_and_nothing_hidden() {
    for judge in [None, Some(Arc::new(NullJudge) as Arc<dyn SemanticJudge>)] {
        let (sift, _, _) = harness(judge);
        let mut nl = message(
            "nl2",
            "weekly@othernews.example",
            "Deals this week",
            "Sale! 50% off. Unsubscribe.",
        );
        nl.headers.list_unsubscribe = Some("<mailto:x>".into());
        let outcome = sift.run(input(nl, PeopleFacts::default())).await.unwrap();
        assert_eq!(
            outcome.classification.disposition,
            Disposition::Unresolved,
            "{}",
            outcome.classification.reason
        );
        assert!(outcome.judge_unavailable);
        assert!(outcome.trace.strategy.contains("judge unavailable"));
        assert!(!outcome.classification.disposition.hides());

        let receipt = message(
            "rc1",
            "orders@shop.example",
            "Your order #1234",
            "Thank you for your order.",
        );
        let mut inp = input(receipt, PeopleFacts::default());
        inp.has_structured_receipt = true;
        let outcome = sift.run(inp).await.unwrap();
        assert_eq!(
            outcome.classification.disposition,
            Disposition::Filed,
            "a fact-only route still files the obvious"
        );
        assert_eq!(
            outcome.classification.mechanism,
            DecisionMechanism::Deterministic
        );
        assert!(outcome
            .classification
            .strata
            .contains(&"Receipts".to_string()));
    }
    // An outage mid-run (the judge answers unavailable) behaves the same for
    // mail that needs a Signal; mail a fact decides is still decided.
    let judge = Arc::new(ScriptedJudge::new());
    judge.set_unavailable(true);
    let (sift, _, _) = harness(Some(judge));
    let mut nl = message(
        "nl3",
        "weekly@othernews.example",
        "Deals this week",
        "Sale! 50% off. Unsubscribe.",
    );
    nl.headers.list_unsubscribe = Some("<mailto:x>".into());
    let outcome = sift.run(input(nl, PeopleFacts::default())).await.unwrap();
    assert_eq!(outcome.classification.disposition, Disposition::Unresolved);
    assert!(outcome.classification.reason.contains("judge unavailable"));
    let review = sift
        .run(input(github_review_request(), PeopleFacts::default()))
        .await
        .unwrap();
    assert_eq!(
        review.classification.disposition,
        Disposition::Find {
            find_type: FindType::Review
        },
        "a review request is decided by its header, judge or no judge"
    );
}

struct FlipRefiner {
    signal: String,
    yes: f64,
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl SignalRefiner for FlipRefiner {
    async fn refine(&self, req: RefineRequest) -> riddle_errors::Result<RefineResponse> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(
            req.message_text.contains("UNTRUSTED MAIL CONTENT"),
            "mail is delimited as untrusted"
        );
        let mut refined = Vec::new();
        for e in &req.signals {
            if e.signal_name == self.signal {
                let mut r = e.clone();
                r.distribution.insert("YES".into(), self.yes);
                r.distribution.insert("NO".into(), 1.0 - self.yes);
                r.calibrated = r.distribution.clone();
                r.selected = "YES".into();
                r.engine_identity = "scripted-llm".into();
                r.cached = false;
                refined.push(r);
            }
        }
        Ok(RefineResponse {
            refined,
            rationale: "the sender is plainly waiting for an answer".into(),
            model_id: "scripted-llm".into(),
            latency_ms: 3,
            input_tokens: 500,
            output_tokens: 40,
            cost_usd: Some(0.001),
        })
    }
    fn identity(&self) -> String {
        "scripted-llm".into()
    }
}

/// §39: an uncertain route escalates; the LLM refines a Signal, never a
/// disposition; the Mesh routes again on the refined value.
#[tokio::test]
async fn escalation_with_a_scripted_refiner_flips_a_signal_and_the_mesh_decides_again() {
    let judge = ScriptedJudge::new()
        .with_override("requires_response", 0.72)
        .with_override("asks_question", 0.3)
        .with_override("requires_action", 0.2)
        .with_override("routine", 0.2)
        .with_override("human_authored", 0.9);
    let refiner = Arc::new(FlipRefiner {
        signal: "requires_response".into(),
        yes: 0.95,
        calls: Default::default(),
    });
    let (mut sift, _, _) = harness(Some(Arc::new(judge)));
    sift.refiner = Some(refiner.clone());
    let m = message(
        "hu1",
        "colleague@partner.example",
        "Re: the plan",
        "Hi Sam, are we still on for the plan? Let me know.",
    );
    let outcome = sift
        .run(input(m.clone(), PeopleFacts::known()))
        .await
        .unwrap();
    assert_eq!(refiner.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        outcome.classification.disposition,
        Disposition::Find {
            find_type: FindType::Respond
        },
        "{}",
        outcome.classification.reason
    );
    assert_eq!(outcome.classification.mechanism, DecisionMechanism::Llm);
    let esc = outcome
        .trace
        .escalation
        .clone()
        .expect("escalation recorded");
    assert!(esc.reason.contains("margin"));
    assert_eq!(esc.refined.len(), 1);
    assert!(outcome.trace.cost.llm_cost_usd.is_some());
    assert!(outcome
        .trace
        .signals
        .iter()
        .any(|s| s.signal_name == "requires_response" && s.score(None) > 0.9));

    // Without a refiner the same message is Unresolved, not guessed.
    let judge = ScriptedJudge::new()
        .with_override("requires_response", 0.72)
        .with_override("asks_question", 0.3)
        .with_override("requires_action", 0.2)
        .with_override("routine", 0.2)
        .with_override("human_authored", 0.9);
    let (sift, _, _) = harness(Some(Arc::new(judge)));
    let outcome = sift.run(input(m, PeopleFacts::known())).await.unwrap();
    assert_eq!(outcome.classification.disposition, Disposition::Unresolved);
    assert!(outcome
        .classification
        .reason
        .contains("no escalation model"));
}

/// §41: a Find from an unknown sender needs a deterministic supporting fact.
#[tokio::test]
async fn a_find_from_an_unknown_sender_needs_deterministic_support() {
    let (sift, stores, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    // The default people rule decides only senders the user knows, so a
    // rule that would make a stranger's ask a Find is written here.
    let mut asks = riddle_mesh::parse::parse(
        "Mesh: Asks\nPriority: 60\nROUTES\nIF requires_response@v1 > .60 → Find: Respond",
    )
    .unwrap();
    asks.status = MeshStatus::Active;
    riddle_store::repo::meshes::upsert(&stores.index(), &asks).unwrap();
    let mut m = message(
        "st1",
        "stranger@unknown.example",
        "Quick question",
        "Hi, can you send me the report? Thanks.",
    );
    m.to = vec![Address::new("someone-else@work.example", None)];
    let outcome = sift
        .run(input(m.clone(), PeopleFacts::first_time()))
        .await
        .unwrap();
    assert_eq!(
        outcome.classification.disposition,
        Disposition::Unresolved,
        "{}",
        outcome.classification.reason
    );
    assert!(outcome
        .classification
        .reason
        .contains("no deterministic support"));
    let mut supported = m;
    supported.to = vec![Address::new("me@work.example", None)];
    supported.headers.authentication.spf = AuthVerdict::Pass;
    let outcome = sift
        .run(input(supported, PeopleFacts::first_time()))
        .await
        .unwrap();
    assert_eq!(
        outcome.classification.disposition,
        Disposition::Find {
            find_type: FindType::Respond
        }
    );
}

/// §17, invariant 49: sent mail is sifted for asks and commitments, Filed,
/// and never a Find.
#[tokio::test]
async fn sent_mail_is_filed_and_updates_thread_state() {
    let (sift, _, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    let mut m = message(
        "se1",
        "me@work.example",
        "Contract",
        "Can you send the signed contract by Friday? I will forward the invoice today.",
    );
    m.is_sent = true;
    m.labels = vec!["SENT".into()];
    let outcome = sift.run(input(m, PeopleFacts::known())).await.unwrap();
    assert_eq!(outcome.classification.disposition, Disposition::Filed);
    assert!(outcome.find.is_none());
    let update = outcome.thread_state_update.expect("thread state update");
    assert!(update.asks_question);
    assert!(update.makes_commitment);
    assert!(update.requests_action);
    assert!(!outcome.trace.facts.deadlines.is_empty());
}

/// §51: a Shadow Mesh or a Dig in its Shadow Period records and shows, but
/// persist applies nothing (no Find proposal, no projection).
#[tokio::test]
async fn a_shadow_dig_records_the_classification_but_proposes_no_find() {
    let (sift, stores, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    let mut inp = input(github_review_request(), PeopleFacts::default());
    inp.dig.shadow = true;
    let mut outcome = sift.run(inp).await.unwrap();
    assert!(outcome.shadow);
    assert!(outcome.classification.shadow);
    let proposal = sift.persist(&mut outcome).unwrap();
    assert!(proposal.is_none());
    let store = stores.dig(&DigId("d1".into())).unwrap();
    assert!(
        repo::classifications::current(&store, &outcome.classification.message_id)
            .unwrap()
            .is_none()
    );
    assert!(
        repo::classifications::current_shadow(&store, &outcome.classification.message_id)
            .unwrap()
            .is_some()
    );
}

/// §46, §50: `move_to_spoil` writes a user classification and a correction;
/// undo restores the prior classification and retracts the correction.
#[tokio::test]
async fn move_to_spoil_records_a_correction_and_undo_restores_and_retracts() {
    let (sift, stores, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    let mut outcome = sift
        .run(input(github_review_request(), PeopleFacts::default()))
        .await
        .unwrap();
    sift.persist(&mut outcome).unwrap();
    let id = outcome.classification.message_id.clone();
    let dig = DigId("d1".into());
    let store = stores.dig(&dig).unwrap();
    repo::messages::upsert(&store, &github_review_request()).unwrap();

    let services = Arc::new(Services::new(
        stores.clone(),
        Arc::new(RecordingSink::default()),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    ));
    let ctx = ActionContext {
        services: services.clone(),
        principal: Principal::User,
        dig_id: Some(dig.clone()),
        idempotency_key: None,
        activity_id: ActivityId::new(),
    };
    let action = riddle_sift::actions::MoveToSpoil;
    let result = action
        .execute(
            &ctx,
            serde_json::json!({ "digId": dig, "messageIds": [id, "d1:missing"] }),
        )
        .await
        .unwrap();
    assert_eq!(
        result.affected.len(),
        2,
        "every message gets an entity ref, even one with no prior classification"
    );
    let current = repo::classifications::current(&store, &id)
        .unwrap()
        .unwrap();
    assert_eq!(current.disposition, Disposition::Spoil);
    assert_eq!(current.mechanism, DecisionMechanism::User);
    let corrections = repo::corrections::for_message(&store, &id).unwrap();
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].kind, CorrectionKind::Move);
    assert_eq!(corrections[0].activity_id, Some(ctx.activity_id.clone()));

    let UndoInfo::Undoable { restore, .. } = result.undo else {
        panic!("undoable")
    };
    let undo_ctx = ActionContext {
        services,
        principal: Principal::User,
        dig_id: Some(dig.clone()),
        idempotency_key: None,
        activity_id: ActivityId::new(),
    };
    let undone = action.undo(&undo_ctx, restore).await.unwrap();
    assert_eq!(undone.affected.len(), 2);
    let restored = repo::classifications::current(&store, &id)
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.disposition,
        Disposition::Find {
            find_type: FindType::Review
        }
    );
    assert!(
        repo::classifications::current(&store, &MessageId("d1:missing".into()))
            .unwrap()
            .is_none(),
        "a message with no prior classification returns to the backlog"
    );
    let corrections = repo::corrections::for_message(&store, &id).unwrap();
    assert!(
        corrections[0].retracted_at.is_some(),
        "undo retracts the correction"
    );

    // set_disposition to a Find creates the Find through riddle-finds.
    let ctx = ActionContext {
        services: undo_ctx.services.clone(),
        principal: Principal::User,
        dig_id: Some(dig.clone()),
        idempotency_key: None,
        activity_id: ActivityId::new(),
    };
    let result = riddle_sift::actions::SetDisposition.execute(&ctx, serde_json::json!({ "digId": dig, "messageId": id, "disposition": "find", "findType": "do" })).await.unwrap();
    assert!(result.value["findId"].is_string());
    assert_eq!(
        repo::corrections::for_message(&store, &id)
            .unwrap()
            .iter()
            .filter(|k| k.kind == CorrectionKind::Retype && k.retracted_at.is_none())
            .count(),
        1
    );
}

/// A projection hook that records what it was handed: the message with the
/// classification (`project_message`), or the classification alone.
#[derive(Default)]
struct RecordingHook {
    with_message: std::sync::Mutex<Vec<(DigId, MessageId, ClassificationId)>>,
    without: std::sync::Mutex<Vec<MessageId>>,
}

impl ProjectionHook for RecordingHook {
    fn project(&self, _dig: &DigId, k: &Classification) -> riddle_errors::Result<()> {
        self.without.lock().unwrap().push(k.message_id.clone());
        Ok(())
    }
    fn project_message(
        &self,
        dig: &DigId,
        m: &Message,
        k: &Classification,
    ) -> riddle_errors::Result<()> {
        self.with_message
            .lock()
            .unwrap()
            .push((dig.clone(), m.id.clone(), k.id.clone()));
        Ok(())
    }
    fn unproject(&self, _dig: &DigId, _id: &MessageId) -> riddle_errors::Result<()> {
        Ok(())
    }
}

/// Persisting a decision is one write (17 Oct 2026): the facts, the
/// classification and the trace land together, the hook gets the message
/// Sift read rather than reading it again, and deciding the message again
/// supersedes the first decision as before.
#[tokio::test]
async fn persist_leaves_facts_classification_and_trace_in_one_write_and_hands_the_hook_the_message()
{
    let (mut sift, stores, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    let hook = Arc::new(RecordingHook::default());
    sift.projection = Some(hook.clone());
    let m = github_review_request();
    let people = PeopleFacts {
        mailing_list: true,
        ..Default::default()
    };
    let mut outcome = sift.run(input(m.clone(), people.clone())).await.unwrap();
    assert_eq!(outcome.message.id, m.id);
    let proposal = sift.persist(&mut outcome).unwrap();
    assert!(proposal.is_some());
    let store = stores.dig(&DigId("d1".into())).unwrap();
    let k = repo::classifications::current(&store, &m.id)
        .unwrap()
        .expect("the current classification");
    assert_eq!(k.id, outcome.classification.id);
    assert_eq!(
        k.disposition,
        Disposition::Find {
            find_type: FindType::Review
        }
    );
    assert_eq!(k.strata, outcome.classification.strata);
    let facts = repo::facts::get(&store, &m.id).unwrap().expect("facts row");
    assert_eq!(facts.sender_domain, outcome.facts.sender_domain);
    let trace = repo::traces::get(&store, &k.trace_id)
        .unwrap()
        .expect("trace row");
    assert_eq!(trace.conclusion, k.disposition);
    assert_eq!(
        trace.timing.persist_ms, outcome.timing.persist_ms,
        "the stored trace carries the persist timing"
    );
    assert!(repo::classifications::current_shadow(&store, &m.id)
        .unwrap()
        .is_none());
    assert!(outcome.superseded.is_empty(), "no `+ Replaces` series");
    assert_eq!(
        hook.with_message.lock().unwrap().as_slice(),
        &[(DigId("d1".into()), m.id.clone(), k.id.clone())]
    );
    assert!(
        hook.without.lock().unwrap().is_empty(),
        "the hook was handed the message, not asked to read it"
    );

    // Decided again: one current decision, the first superseded.
    let mut again = sift.run(input(m.clone(), people)).await.unwrap();
    sift.persist(&mut again).unwrap();
    assert_eq!(
        repo::classifications::history(&store, &m.id).unwrap().len(),
        2
    );
    assert_eq!(
        repo::classifications::current(&store, &m.id)
            .unwrap()
            .unwrap()
            .id,
        again.classification.id,
        "the second decision is the current one"
    );
    assert_eq!(repo::traces::for_message(&store, &m.id).unwrap().len(), 2);
}

/// The named lists and the Signal registry are read once for the pass a
/// `Sift` is built for, like the Meshes (17 Oct 2026): a list added or a
/// Signal promoted while the pass runs is used by the next pass.
#[tokio::test]
async fn the_lists_and_the_registry_are_read_once_per_pass_and_an_edit_waits_for_the_next_pass() {
    let (sift, stores, _) = harness(Some(Arc::new(ScriptedJudge::new())));
    let index = stores.index();
    sift.run(input(github_comment(), PeopleFacts::default()))
        .await
        .unwrap();
    let snapshot = sift.pass_snapshot().unwrap();
    assert!(!snapshot.named.exists("banks"));
    assert_eq!(
        snapshot
            .registry
            .resolve_active("review_requested")
            .unwrap()
            .version,
        1
    );

    // Mid-pass edits: a new list and a promoted Signal.
    repo::lists::put(
        &index,
        &NamedListDef {
            name: "banks".into(),
            description: String::new(),
            entries: vec!["alerts@bank.example".into()],
            pack: None,
            kinds: vec![],
        },
    )
    .unwrap();
    let registry = riddle_signals::Registry::new(index.clone());
    registry
        .propose(
            Signal::noul(
                "review_requested",
                "The recipient is named as a reviewer of a change.",
            ),
            CreatedBy::User,
        )
        .unwrap();
    registry
        .promote(
            "review_requested",
            2,
            riddle_signals::PromoteOptions {
                report: None,
                override_untested: true,
            },
        )
        .unwrap();
    assert_eq!(
        registry.resolve_active("review_requested").unwrap().version,
        2
    );

    sift.run(input(github_comment(), PeopleFacts::default()))
        .await
        .unwrap();
    let same = sift.pass_snapshot().unwrap();
    assert!(!same.named.exists("banks"), "this pass keeps its lists");
    assert_eq!(
        same.registry
            .resolve_active("review_requested")
            .unwrap()
            .version,
        1,
        "this pass keeps its registry"
    );

    // The next pass builds its own Sift and sees both.
    let next = Sift::new(
        stores.clone(),
        Arc::new(SystemClock),
        Arc::new(RecordingSink::default()),
        BuildStamp::from_env_vars("0.1.0-test", None, None, None, "test"),
    );
    let fresh = next.pass_snapshot().unwrap();
    assert!(fresh.named.exists("banks"));
    assert_eq!(
        fresh
            .registry
            .resolve_active("review_requested")
            .unwrap()
            .version,
        2
    );
}
