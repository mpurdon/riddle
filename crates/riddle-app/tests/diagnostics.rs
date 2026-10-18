//! What the diagnostics say about a pass, a paused judge and dropped
//! bookkeeping. The judge's breaker opened and closed for a fortnight while
//! every Dig said "Sifting", swallowed failures left one warn line each in
//! a ring buffer, and the export was fifty of those lines (17 Oct 2026).

use riddle_app::ops::AppDigStatus;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::sync::Arc;

mod common;

/// A judge whose health says what the test wants and whose calls fail as
/// an open breaker's do: `JudgeUnavailable`, which Sift turns into
/// Unresolved and nothing hidden.
struct PausedJudge(ServiceHealth);

#[async_trait::async_trait]
impl SemanticJudge for PausedJudge {
    async fn evaluate(
        &self,
        _state: &JudgeState,
        _questions: &[JudgeQuestion],
    ) -> riddle_errors::Result<Vec<JudgeAnswer>> {
        Err(riddle_errors::RiddleError::judge_unavailable(
            "circuit is open",
        ))
    }
    fn limits(&self) -> JudgeLimits {
        JudgeLimits {
            max_state_tokens: 32_000,
            max_request_tokens: 64_000,
            max_questions: 64,
            requests_per_minute: 0,
        }
    }
    fn identity(&self) -> JudgeIdentity {
        JudgeIdentity {
            provider: "test".into(),
            model_id: "paused".into(),
            model_version: "0".into(),
            price_per_mtok: None,
        }
    }
    fn health(&self) -> ServiceHealth {
        self.0.clone()
    }
}

/// A new, unclassified message in the Dig, so a pass has something to do.
fn fresh_message(app: &riddle_app::App, dig: &Dig, id: &str) -> MessageId {
    let store = app.stores.dig(&dig.id).unwrap();
    let mut m = riddle_mail::fixture::blank_message(dig.id.as_str(), id);
    m.from = Address::new(format!("{id}@shop.example"), None);
    m.subject = format!("Order {id} has shipped");
    m.body_text = Some("Your order is on its way.".into());
    m.has_body = true;
    repo::messages::upsert(&store, &m).unwrap();
    m.id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fixture_pass_reports_its_elapsed_time_judge_time_and_store_calls() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    fresh_message(&app, &dig, "diag1");
    let report = app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(report.processed, 1, "{report:?}");
    assert!(report.store_reads > 0, "{report:?}");
    assert!(report.store_writes > 0, "{report:?}");
    assert!(report.elapsed_ms >= report.judge_ms, "{report:?}");
    assert!(!report.judge_unavailable);
    let line = riddle_observability::tail(
        500,
        Some("sift"),
        Some(log::Level::Info),
        Some(&format!("pass {}:", dig.id)),
    )
    .pop()
    .expect("the pass logged a line");
    assert!(
        line.message.contains("failed 0, judge unavailable false"),
        "{}",
        line.message
    );
    assert!(line.message.contains("store reads"), "{}", line.message);
    assert!(line.message.contains("(judge "), "{}", line.message);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_judge_breaker_makes_a_dig_with_mail_waiting_degraded_with_a_retry_time() {
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    fresh_message(&app, &dig, "waiting");
    let status = |app: &Arc<riddle_app::App>| {
        AppDigStatus(app.clone())
            .status_of(&app.dig(&dig.id).unwrap())
            .unwrap()
    };
    assert!(
        matches!(status(&app), DigStatus::Sifting { .. }),
        "mail waits and the judge is fine"
    );
    let until = chrono::Utc::now() + chrono::Duration::minutes(5);
    let paused = ServiceHealth {
        breaker: BreakerHealth::Open { until },
        last_success_at: Some(chrono::Utc::now() - chrono::Duration::minutes(3)),
        last_failure_at: Some(chrono::Utc::now() - chrono::Duration::seconds(30)),
        last_failure_kind: Some("judge_unavailable".into()),
    };
    *app.services.judge.write() = Some(Arc::new(PausedJudge(paused.clone())));
    match status(&app) {
        DigStatus::Degraded { reason } => {
            assert!(
                reason.starts_with("judge paused after repeated failures, retrying at "),
                "{reason}"
            );
            assert!(reason.contains(&app.local_clock(&until)), "{reason}");
            assert!(reason.ends_with("; nothing hidden"), "{reason}");
        }
        other => panic!("expected Degraded, got {other:?}"),
    }
    let tc = app.task_centre().unwrap();
    assert_eq!(tc.judge, Some(paused));
    assert!(
        tc.llm.is_none(),
        "no LLM is configured under the null provider"
    );
    let probe = riddle_app::dispatch::call(&app, "probe_judge", json!({}))
        .await
        .unwrap();
    assert_eq!(probe["ok"], false);
    assert_eq!(probe["health"]["breaker"]["state"], "open");
    assert_eq!(probe["health"]["lastFailureKind"], "judge_unavailable");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pass_under_a_paused_judge_says_so_in_its_activity_note() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    *app.services.judge.write() = Some(Arc::new(PausedJudge(ServiceHealth::default())));
    fresh_message(&app, &dig, "paused1");
    let report = app.sift_pass(&dig, 100, None).await.unwrap();
    assert!(report.judge_unavailable, "{report:?}");
    let page = repo::activities::list(
        &app.stores.index(),
        &repo::activities::ActivityFilter {
            action: Some("sift_classify".into()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    let note = page
        .items
        .iter()
        .find(|a| a.inputs["judgeUnavailable"] == true)
        .and_then(|a| a.provenance.as_ref())
        .and_then(|p| p.note.clone())
        .expect("a sift_classify Activity from the paused pass");
    assert!(note.contains("The judge was unavailable"), "{note}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swallowed_failure_is_counted_and_shows_in_dev_info() {
    let (app, _ops) = common::boot_scripted(false, |_| {}).await;
    let err = riddle_errors::RiddleError::store("disk full");
    riddle_observability::swallowed!(
        riddle_observability::Category::App,
        "diagnostics_test_site",
        err
    );
    let info = riddle_app::dispatch::call(&app, "dev_info", json!({}))
        .await
        .unwrap();
    let row = info["swallowed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["site"] == "diagnostics_test_site")
        .expect("the site is in dev_info")
        .clone();
    assert_eq!(row["category"], "app");
    assert_eq!(row["count"], 1);
    assert!(row["lastError"].as_str().unwrap().contains("disk full"));
    assert!(row["lastAt"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_diagnostics_export_carries_every_section_and_no_address() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    let out = riddle_app::dispatch::call(&app, "export_diagnostics", json!({}))
        .await
        .unwrap();
    for key in [
        "exportedAt",
        "redaction",
        "runtime",
        "taskCentre",
        "traceStats",
        "judge",
        "llm",
        "digs",
        "settings",
        "swallowed",
        "warnings",
    ] {
        assert!(out.get(key).is_some(), "section {key}");
    }
    assert_eq!(out["judge"]["identity"]["provider"], "scripted");
    assert_eq!(out["judge"]["health"]["breaker"]["state"], "unknown");
    assert!(out["llm"].is_null(), "the null provider is no LLM");
    assert!(out["runtime"]["swallowed"].is_array());
    assert!(out["traceStats"]["count"].as_u64().unwrap() > 0);
    let row = &out["digs"][0];
    assert_eq!(row["id"], json!(dig.id));
    assert!(row["status"]["state"].is_string());
    assert!(row.get("email").is_none(), "addresses stay out");
    let client = out["settings"]["gmail"]["clientId"].as_str().unwrap();
    assert!(client == "configured" || client == "absent", "{client}");
    assert_eq!(out["taskCentre"]["digs"][0]["digId"], json!(dig.id));
    let text = out.to_string();
    assert!(
        !text.contains("me@work.example"),
        "the export carries no address"
    );
}

/// Claim: an install that was never given a judge key is a setup to finish,
/// not an outage. With mail waiting the account says what Riddle does
/// meanwhile, the probe says "not set up" apart from a failure, and the
/// Summary carries the same words.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dig_with_mail_waiting_and_no_judge_key_says_it_sorts_by_rules_and_the_probe_says_not_set_up(
) {
    let (app, ops) = common::boot_scripted(false, |s| {
        s.shadow_period_days = 0;
        s.judge.provider = riddle_settings::JudgeProvider::Null;
    })
    .await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    fresh_message(&app, &dig, "nokey");
    assert!(app.judge_not_set_up());
    match AppDigStatus(app.clone())
        .status_of(&app.dig(&dig.id).unwrap())
        .unwrap()
    {
        DigStatus::Degraded { reason } => assert_eq!(reason, riddle_app::ops::NO_JUDGE_KEY),
        other => panic!("expected Degraded, got {other:?}"),
    }
    let probe = riddle_app::dispatch::call(&app, "probe_judge", json!({}))
        .await
        .unwrap();
    assert_eq!(probe["ok"], false);
    assert_eq!(probe["notSetUp"], true);
    assert!(
        probe["message"].as_str().unwrap().starts_with("Not set up"),
        "{probe}"
    );
    app.refresh_dig_status(&dig.id);
    let summary = app
        .summary(chrono::Utc::now() - chrono::Duration::hours(24), None)
        .unwrap();
    assert_eq!(
        summary.degraded,
        vec![format!("Work: {}", riddle_app::ops::NO_JUDGE_KEY)]
    );
}
