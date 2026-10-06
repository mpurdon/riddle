//! Expedition runs against the scripted model and scripted reads.

use riddle_actions::policy::AutonomyPolicy;
use riddle_actions::{Runtime, RuntimeBuilder, Services};
use riddle_agent::{ScriptedReads, ToolLoop};
use riddle_ai::{PromptStore, ScriptedLlm, ScriptedResponse};
use riddle_core::*;
use riddle_expeditions::runner::Checkpoint;
use riddle_expeditions::{ExpeditionConfig, ExpeditionRunner};
use riddle_store::{repo, Stores};
use serde_json::json;
use std::sync::Arc;

struct Harness {
    runner: Arc<ExpeditionRunner>,
    llm: Arc<ScriptedLlm>,
    reads: Arc<ScriptedReads>,
    sink: Arc<RecordingSink>,
    stores: Arc<Stores>,
    runtime: Arc<Runtime>,
}

fn dig(id: &str, privacy: PrivacyMode) -> Dig {
    Dig {
        id: DigId::from(id),
        name: id.into(),
        email: format!("{id}@example.com"),
        provider: ProviderKind::Fixture,
        group: None,
        privacy,
        projection: ProjectionMode::default(),
        status: DigStatus::Synced,
        shadow_started_at: None,
        created_at: chrono::Utc::now(),
        disabled: false,
        mailbox: None,
    }
}

fn harness(script: Vec<ScriptedResponse>, reads: ScriptedReads) -> Harness {
    let stores = Arc::new(Stores::in_memory().unwrap());
    repo::digs::upsert(&stores.index(), &dig("d1", PrivacyMode::CloudReasoning)).unwrap();
    let sink = Arc::new(RecordingSink::default());
    let services = Arc::new(Services::new(
        stores.clone(),
        sink.clone(),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    ));
    let runtime = RuntimeBuilder::new(services.clone())
        .registry(|r| {
            for a in riddle_expeditions::actions() {
                r.register(a);
            }
        })
        .policy(AutonomyPolicy {
            default: Autonomy::Act,
            ..Default::default()
        })
        .build();
    let llm = Arc::new(ScriptedLlm::new(script));
    let reads = Arc::new(reads);
    let tool_loop = Arc::new(ToolLoop::new(
        llm.clone(),
        runtime.clone(),
        reads.clone(),
        sink.clone(),
    ));
    let runner = ExpeditionRunner::new(
        tool_loop,
        stores.clone(),
        sink.clone(),
        Arc::new(PromptStore::embedded()),
        Arc::new(SystemClock),
        ExpeditionConfig::default(),
    );
    runner.register(&services);
    Harness {
        runner,
        llm,
        reads,
        sink,
        stores,
        runtime,
    }
}

fn page(ids: &[&str]) -> serde_json::Value {
    json!({"items": ids.iter().map(|i| json!({"id": i, "digId": "d1", "subject": format!("Receipt {i}"), "snippet": "Your payment", "disposition": {"kind": "filed"}})).collect::<Vec<_>>(), "total": ids.len()})
}

fn budget(tool_calls: u32) -> ExpeditionBudget {
    ExpeditionBudget {
        cost_usd: 1.0,
        seconds: 600,
        tool_calls,
    }
}

#[tokio::test]
async fn an_expedition_that_exhausts_its_tool_call_budget_finishes_partial_with_a_checkpoint_and_findings(
) {
    let h = harness(
        vec![
            ScriptedLlm::text(
                r#"{"steps":["Find Adobe receipts","Find the cancellation","Compare dates"]}"#,
            ),
            // Step 1: one search, then a summary.
            ScriptedLlm::tool_call("search", json!({"query": "from:adobe receipt"})),
            ScriptedLlm::text("Found receipts d1:1 and d1:2 dated after March."),
            // Step 2: one search (budget now spent), then the forced conclusion.
            ScriptedLlm::tool_call("search", json!({"query": "adobe cancel"})),
            ScriptedLlm::text("Cancellation confirmed in d1:3 on 2026-03-01."),
            // Conclusion.
            ScriptedLlm::text(
                r#"{"findings":["Adobe charged twice after the cancellation on 2026-03-01."],"evidence":["d1:1","d1:2","d1:3"]}"#,
            ),
        ],
        ScriptedReads::new()
            .on("search", page(&["d1:1", "d1:2"]))
            .on("search", page(&["d1:3"])),
    );
    let e = h
        .runner
        .create(
            "Did Adobe keep charging after I cancelled?",
            ExpeditionScope {
                dig_ids: vec![DigId::from("d1")],
                ..Default::default()
            },
            budget(2),
        )
        .unwrap();
    let done = h.runner.run(&e.id).await.unwrap();

    assert_eq!(done.status, ExpeditionStatus::Partial);
    assert_eq!(done.plan.len(), 3);
    assert!(done.plan[0].done && done.plan[1].done && !done.plan[2].done);
    let cp = Checkpoint::from_value(&done.checkpoint);
    assert_eq!(cp.completed, 2);
    assert_eq!(cp.notes.len(), 2);
    assert!(cp.read_untrusted, "search results are mail content");
    assert_eq!(done.spent.tool_calls, 2);
    assert!(done.error.as_deref().unwrap().contains("tool calls"));
    assert_eq!(
        done.findings,
        vec!["Adobe charged twice after the cancellation on 2026-03-01."]
    );
    assert!(done.evidence.iter().any(|r| r.id == "d1:3"));
    assert!(done.finished_at.is_some());

    // Persisted, and the store agrees.
    let stored = repo::expeditions::get(&h.stores.index(), &e.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, ExpeditionStatus::Partial);
    assert!(repo::expeditions::resumable(&h.stores.index())
        .unwrap()
        .is_empty());
    // Progress events were emitted and the model saw the budget note before concluding.
    let events = h.sink.take();
    assert!(
        events
            .iter()
            .filter(|e| matches!(e, RiddleEvent::ExpeditionChanged { .. }))
            .count()
            >= 4
    );
    let reqs = h.llm.requests();
    assert!(reqs
        .iter()
        .any(|r| r.messages.iter().any(|m| m.text().contains("Budget note"))));
    assert!(reqs.iter().all(|r| r.task == UsageTask::Expedition));
    // The Expedition ran as the Expedition principal and could not see mutating tools.
    assert!(reqs[1]
        .tools
        .iter()
        .all(|t| !t.name.starts_with("cancel_expedition")));
    // create_find is registered: the finding became a Find of type Know.
    assert!(events.iter().any(|e| matches!(e, RiddleEvent::FindCreated { find } if find.find_type == FindType::Know && find.sources.len() == 3)));
    let finds = repo::finds::list(
        &h.stores.index(),
        &Default::default(),
        &PageRequest::default(),
    )
    .unwrap();
    assert_eq!(finds.items.len(), 1);
    assert_eq!(
        finds.items[0].reason,
        "Expedition: Did Adobe keep charging after I cancelled?"
    );
    let _ = (&h.reads, &h.runtime);
}

#[tokio::test]
async fn resume_continues_from_the_checkpoint_after_a_restart() {
    // Only the remaining step and the conclusion are scripted: a repeat of
    // step 1 would exhaust the script and fail the test.
    let h = harness(
        vec![
            ScriptedLlm::tool_call("get_message", json!({"digId": "d1", "messageId": "d1:7"})),
            ScriptedLlm::text("The hotel confirmation is d1:7, Lisbon, 12-15 May."),
            ScriptedLlm::text(
                r#"{"findings":["Hotel: Lisbon 12-15 May (d1:7)","Flights: TAP on 12 May (d1:5)"],"evidence":["d1:5","d1:7"]}"#,
            ),
        ],
        ScriptedReads::new().on(
            "get_message",
            json!({"id": "d1:7", "subject": "Booking confirmed", "bodyText": "Lisbon 12-15 May"}),
        ),
    );
    // A run that was interrupted after step 1 of 2.
    let mut e = h
        .runner
        .create(
            "Find my Portugal reservations",
            ExpeditionScope {
                dig_ids: vec![DigId::from("d1")],
                ..Default::default()
            },
            budget(10),
        )
        .unwrap();
    e.status = ExpeditionStatus::Running;
    e.plan = vec![
        PlanStep {
            n: 1,
            text: "Find flights".into(),
            done: true,
        },
        PlanStep {
            n: 2,
            text: "Find hotels".into(),
            done: false,
        },
    ];
    e.checkpoint = json!(Checkpoint {
        completed: 1,
        notes: vec!["Flights: TAP on 12 May (d1:5).".into()],
        evidence: vec!["d1:5".into()],
        read_untrusted: true
    });
    e.spent = ExpeditionBudget {
        cost_usd: 0.01,
        seconds: 30,
        tool_calls: 3,
    };
    repo::expeditions::upsert(&h.stores.index(), &e).unwrap();
    assert_eq!(
        repo::expeditions::resumable(&h.stores.index())
            .unwrap()
            .len(),
        1
    );

    let done = h.runner.resume(&e.id).await.unwrap();
    assert_eq!(done.status, ExpeditionStatus::Completed);
    assert!(done.plan.iter().all(|s| s.done));
    let cp = Checkpoint::from_value(&done.checkpoint);
    assert_eq!(cp.completed, 2);
    assert_eq!(
        cp.notes[0], "Flights: TAP on 12 May (d1:5).",
        "the earlier note survived the restart"
    );
    assert_eq!(done.spent.tool_calls, 4, "3 before the restart, 1 after");
    assert_eq!(done.findings.len(), 2);
    assert!(
        done.evidence.iter().any(|r| r.id == "d1:5")
            && done.evidence.iter().any(|r| r.id == "d1:7")
    );
    assert_eq!(h.llm.calls(), 3, "no re-planning, no repeat of step 1");
    // The step prompt carried the evidence so far, and the principal kept read_untrusted from the checkpoint.
    let first = &h.llm.requests()[0];
    assert!(first.messages[0]
        .text()
        .contains("Step 1: Flights: TAP on 12 May"));
    assert!(first.messages[0]
        .text()
        .contains("Current step 2 of 2: Find hotels"));
    assert!(first.system.contains("1. [done] Find flights"));
    assert_eq!(h.reads.calls().len(), 1);
    assert!(repo::expeditions::resumable(&h.stores.index())
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn start_and_cancel_through_the_actions() {
    let h = harness(
        vec![
            ScriptedLlm::text(r#"{"steps":["one","two"]}"#),
            ScriptedLlm::text("step one done"),
        ],
        ScriptedReads::new(),
    );
    // Invalid budget is refused before anything is written.
    let bad = h.runtime.invoke_as_user("start_expedition", json!({"goal": "x", "scope": {}, "budget": {"costUsd": 0, "seconds": 0, "toolCalls": 0}})).await.unwrap_err();
    assert_eq!(bad.kind, riddle_errors::ErrorKind::Invalid);

    // A planned Expedition that is not running cancels at once.
    let e = h
        .runner
        .create("cancel me", ExpeditionScope::default(), budget(5))
        .unwrap();
    let out = h
        .runtime
        .invoke_as_user("cancel_expedition", json!({"expeditionId": e.id}))
        .await
        .unwrap();
    assert_eq!(out.result["status"], "cancelled");
    assert!(out.activity_id.is_some(), "a cancellation is Activity");
    assert_eq!(
        repo::expeditions::get(&h.stores.index(), &e.id)
            .unwrap()
            .unwrap()
            .status,
        ExpeditionStatus::Cancelled
    );
    assert!(
        h.runtime
            .invoke_as_user("cancel_expedition", json!({"expeditionId": e.id}))
            .await
            .is_err(),
        "cancelling twice conflicts"
    );

    // start_expedition persists, spawns, and the run finishes on its own.
    let out = h.runtime.invoke_as_user("start_expedition", json!({"goal": "find things", "scope": {"digIds": ["d1"]}, "budget": {"costUsd": 1.0, "seconds": 60, "toolCalls": 3}})).await.unwrap();
    let id = ExpeditionId::from(out.result["id"].as_str().unwrap());
    for _ in 0..200 {
        let e = repo::expeditions::get(&h.stores.index(), &id)
            .unwrap()
            .unwrap();
        if matches!(
            e.status,
            ExpeditionStatus::Completed
                | ExpeditionStatus::Partial
                | ExpeditionStatus::Failed
                | ExpeditionStatus::Cancelled
        ) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let e = repo::expeditions::get(&h.stores.index(), &id)
        .unwrap()
        .unwrap();
    // The script ran out during step two, so the run failed honestly rather than hanging.
    assert!(
        matches!(
            e.status,
            ExpeditionStatus::Failed | ExpeditionStatus::Partial | ExpeditionStatus::Completed
        ),
        "{:?}",
        e.status
    );
    assert_eq!(e.plan.len(), 2);
    assert!(e.plan[0].done);
    let events = h.sink.take();
    assert!(events.iter().any(|e| matches!(e, RiddleEvent::ActivityAppended { activity } if activity.action == "start_expedition")));
}

#[tokio::test]
async fn a_local_only_dig_cannot_be_put_in_scope() {
    let h = harness(vec![], ScriptedReads::new());
    repo::digs::upsert(&h.stores.index(), &dig("work", PrivacyMode::LocalOnly)).unwrap();
    let err = h
        .runner
        .create(
            "read work mail",
            ExpeditionScope {
                dig_ids: vec![DigId::from("work")],
                ..Default::default()
            },
            budget(5),
        )
        .unwrap_err();
    assert_eq!(err.kind, riddle_errors::ErrorKind::PolicyDenied);
    assert_eq!(h.llm.calls(), 0);
}
