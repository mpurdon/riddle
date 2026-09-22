//! Live tests: the fixture Dig sifted by the real judge and the real LLM.
//! `#[ignore]`; each states the claim it proves. Nothing prints a secret.

use riddle_app::ai::AiConfig;
use riddle_app::domains::JudgeConfig;
use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_store::repo;
use riddle_sync::engine::CancelFlag;
use serde_json::json;

/// Claim: with the Jev key resolved from SSM, the pipeline classifies real
/// fixture mail through the judge, caches evaluations, and records cost.
#[tokio::test]
#[ignore]
async fn live_jev_sifts_the_fixture_and_records_usage() {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "live",
    )))
    .unwrap();
    app.attach_domains(None).unwrap();
    let identity = app
        .attach_judge(&JudgeConfig::default())
        .await
        .expect("judge attached from SSM");
    assert!(identity.starts_with("jev:"));
    app.runtime.arm(true);
    let out = app
        .runtime
        .invoke_as_user(
            "add_fixture_dig",
            json!({ "name": "Work", "email": "matt@example.com", "seed": 11, "count": 40 }),
        )
        .await
        .unwrap();
    let dig: Dig = serde_json::from_value(out.result).unwrap();
    let started = std::time::Instant::now();
    let report = app
        .full_pass(&dig.id, 500, &CancelFlag::new())
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert!(!report.sift.judge_unavailable, "judge stayed available");
    assert_eq!(report.sift.remaining, 0);
    let store = app.stores.dig(&dig.id).unwrap();
    let counts = repo::classifications::counts(&store, None, false).unwrap();
    let usage =
        repo::usage::summary(&store, &(chrono::Utc::now() - chrono::Duration::hours(1))).unwrap();
    let stats =
        repo::traces::stats(&store, &(chrono::Utc::now() - chrono::Duration::hours(1))).unwrap();
    println!("LIVE JEV: {} messages in {:.1}s; dispositions {:?}; judge calls {} ({} tokens, ${:.4}); avg judge {:.0} ms; fresh {} cached {}",
        report.sift.processed, elapsed.as_secs_f64(), counts, usage.calls, usage.input_tokens, usage.priced_usd, stats.avg_judge_ms, report.sift.fresh_signals, report.sift.cached_signals);
    assert!(usage.calls > 0);
    assert!(usage.priced_usd > 0.0, "Jev is priced");
    // Re-sifting the same messages is served from the cache: no new judge calls.
    let ids = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(365)),
        5,
    )
    .unwrap();
    let before = usage.calls;
    for id in ids {
        let m = repo::messages::get(&store, &dig.id, &id).unwrap().unwrap();
        let input = app.sift_input(&dig, m).unwrap();
        let sift = riddle_sift::Sift::from_services(&app.services);
        let outcome = sift.run(input).await.unwrap();
        assert_eq!(outcome.fresh, 0, "all Signals cached");
    }
    let after = repo::usage::summary(&store, &(chrono::Utc::now() - chrono::Duration::hours(1)))
        .unwrap()
        .calls;
    assert_eq!(after, before);
}

/// Claim: on a CloudReasoning Dig, an Escalate route reaches Bedrock, the
/// refiner returns refined Signals (never a disposition), and the trace
/// records the escalation with its cost.
#[tokio::test]
#[ignore]
async fn live_bedrock_escalation_refines_signals_and_is_traced() {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "live",
    )))
    .unwrap();
    app.attach_domains(None).unwrap();
    app.attach_ai(&AiConfig::default());
    app.attach_judge(&JudgeConfig::default())
        .await
        .expect("judge");
    app.runtime.arm(true);
    let out = app.runtime.invoke_as_user("add_fixture_dig", json!({ "name": "Work", "email": "matt@example.com", "seed": 3, "count": 25, "privacy": "cloud_reasoning" })).await.unwrap();
    let mut dig: Dig = serde_json::from_value(out.result).unwrap();
    if dig.privacy != PrivacyMode::CloudReasoning {
        dig.privacy = PrivacyMode::CloudReasoning;
        repo::digs::upsert(&app.stores.index(), &dig).unwrap();
    }
    let report = app
        .full_pass(&dig.id, 500, &CancelFlag::new())
        .await
        .unwrap();
    let store = app.stores.dig(&dig.id).unwrap();
    let traces = repo::traces::recent(&store, 200).unwrap();
    let escalated: Vec<&DecisionTrace> = traces.iter().filter(|t| t.escalation.is_some()).collect();
    let counts = repo::classifications::counts(&store, None, false).unwrap();
    let usage = app
        .usage_summary(chrono::Utc::now() - chrono::Duration::hours(1))
        .unwrap();
    println!("LIVE BEDROCK: {} sifted; {} escalated; dispositions {:?}; usage {} calls, priced ${:.4}, unpriced {}; by provider {:?}",
        report.sift.processed, escalated.len(), counts, usage.calls, usage.priced_usd, usage.unpriced_calls, usage.by_provider);
    assert!(!escalated.is_empty(), "at least one message escalated");
    for t in escalated.iter().take(3) {
        let e = t.escalation.as_ref().unwrap();
        println!(
            "ESCALATION model={} reason={:?} refined={} latency={}ms rationale={:?}",
            e.model_id,
            e.reason,
            e.refined.len(),
            e.latency_ms,
            e.rationale.chars().take(300).collect::<String>()
        );
    }
    for t in &escalated {
        let e = t.escalation.as_ref().unwrap();
        assert!(!e.refined.is_empty(), "refined Signals present");
        assert!(!e.rationale.is_empty());
        assert!(e.model_id.contains("haiku"));
        assert!(t.cost.llm_input_tokens > 0);
    }
}
