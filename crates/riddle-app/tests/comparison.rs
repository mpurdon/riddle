//! Model comparison (Developer → Models): several models answer the same
//! escalation and the same Director question; the person's own decisions
//! score the escalations, the person's pick scores the Director, and
//! nothing a comparison produces is applied. The scripted model stands in
//! for every candidate.

use riddle_ai::scripted::ScriptedLlm;
use riddle_core::*;
use riddle_store::repo;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

mod common;

const REFINED: &str = r#"{"signals": [], "rationale": "Reads as an order notice."}"#;

async fn wait_for(mut done: impl FnMut() -> bool) {
    for _ in 0..100 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("comparison did not finish");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn models_are_compared_on_your_decisions_and_nothing_is_applied() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    repo::digs::upsert(
        &app.stores.index(),
        &Dig {
            privacy: PrivacyMode::CloudReasoning,
            ..app.dig(&dig.id).unwrap()
        },
    )
    .unwrap();
    let store = app.stores.dig(&dig.id).unwrap();
    let id = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        1,
    )
    .unwrap()
    .remove(0);
    // The person's decision: the right answer the models are scored against.
    app.runtime
        .invoke_as_user(
            "set_disposition",
            json!({ "digId": dig.id, "messageId": id, "disposition": { "kind": "filed" } }),
        )
        .await
        .unwrap();

    let llm = Arc::new(ScriptedLlm::new(vec![]));
    let ai = app.attach_ai_with(
        &riddle_app::ops::ai_config(&riddle_settings::Settings::dev_defaults()),
        Some(llm.clone()),
    );
    for _ in 0..2 {
        llm.push(ScriptedLlm::text(REFINED));
    }
    let models = vec!["model-a".to_string(), "model-b".to_string()];
    // Decided mail comes first; the hard cases only fill a larger count.
    let n = app.start_escalation_benchmark(models.clone(), 1).unwrap();
    assert_eq!(n, 1);
    wait_for(|| {
        app.comparison_rows(10)
            .unwrap()
            .first()
            .is_some_and(|r| r.outputs.len() == 2)
    })
    .await;
    let row = app.comparison_rows(10).unwrap().remove(0);
    assert_eq!(row.message_id, id, "the decided message is compared first");
    assert_eq!(row.truth, Some(Disposition::Filed));
    for o in &row.outputs {
        assert!(o.error.is_none(), "{:?}", o.error);
        assert!(o.disposition.is_some());
        assert_eq!(o.text, "Reads as an order notice.");
    }
    let requests = llm.requests();
    assert!(requests.iter().all(|r| r.task == UsageTask::Comparison));
    let mut asked: Vec<_> = requests.iter().filter_map(|r| r.model_id.clone()).collect();
    asked.sort();
    assert_eq!(asked, models);
    let scores = app.comparison_overview().unwrap().scores;
    for m in &models {
        let s = scores
            .iter()
            .find(|s| &s.model_id == m && s.task == BenchTask::Escalation)
            .unwrap();
        assert_eq!((s.runs, s.judged), (1, 1));
    }
    // Recorded, never applied: the person's decision stands.
    let k = repo::classifications::current(&store, &id)
        .unwrap()
        .unwrap();
    assert_eq!(
        (k.disposition, k.mechanism),
        (Disposition::Filed, DecisionMechanism::User)
    );

    // The Director: a past question replayed on each model, then picked.
    llm.push(ScriptedLlm::text("You got three receipts this week."));
    ai.director
        .send_and_wait(riddle_agent::director::SendRequest {
            message: "How many receipts did I get this week?".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    llm.push(ScriptedLlm::text("Three."));
    llm.push(ScriptedLlm::text("Three receipts, from two shops."));
    assert_eq!(app.start_director_comparison(models.clone(), 5).unwrap(), 1);
    wait_for(|| {
        app.director_comparisons(10)
            .unwrap()
            .first()
            .is_some_and(|q| q.outputs.len() == 2)
    })
    .await;
    let q = app.director_comparisons(10).unwrap().remove(0);
    assert_eq!(q.question, "How many receipts did I get this week?");
    app.runtime
        .invoke_as_user(
            "pick_director_answer",
            json!({ "questionKey": q.question_key, "modelId": "model-b" }),
        )
        .await
        .unwrap();
    assert_eq!(
        app.director_comparisons(10).unwrap()[0]
            .preferred
            .as_deref(),
        Some("model-b")
    );
    let picked = app
        .comparison_overview()
        .unwrap()
        .scores
        .into_iter()
        .find(|s| s.model_id == "model-b" && s.task == BenchTask::Director)
        .unwrap();
    assert_eq!(picked.preferred, 1);
}

/// Challengers: once the access check finds a model usable, it answers
/// every real escalation beside the production model, recorded as Shadow;
/// a model the account may not call yet is checked and left out.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn usable_challengers_answer_beside_a_real_escalation() {
    let (app, ops) = common::boot_scripted(false, |s| {
        s.comparison.roster = vec!["challenger-a".into(), "not-enabled-b".into()];
    })
    .await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    let store = app.stores.dig(&dig.id).unwrap();
    let id = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        1,
    )
    .unwrap()
    .remove(0);
    let llm = Arc::new(ScriptedLlm::new(vec![]));
    app.attach_ai_with(
        &riddle_app::ops::ai_config(&riddle_settings::Settings::dev_defaults()),
        Some(llm.clone()),
    );
    // The access check: the current models and challenger-a answer; b is refused.
    let current = riddle_settings::Settings::dev_defaults().llm;
    let mut order: Vec<String> = vec![current.escalation_model.clone()];
    if current.director_model != current.escalation_model {
        order.insert(0, current.director_model.clone());
    }
    order.extend(["challenger-a".to_string(), "not-enabled-b".to_string()]);
    for m in &order {
        llm.push(if m == "not-enabled-b" {
            ScriptedLlm::error(riddle_errors::RiddleError::llm_unavailable(
                "anthropic.claude-x is not available for this account",
            ))
        } else {
            ScriptedLlm::text("OK")
        });
    }
    let models = app.probe_models().await.unwrap();
    let access = |m: &str| models.iter().find(|x| x.model_id == m).unwrap().access;
    assert_eq!(access("challenger-a"), ModelAccess::Usable);
    assert_eq!(access("not-enabled-b"), ModelAccess::NotEnabled);

    // A real escalation on this message: the usable challengers that are not
    // the production model answer it too.
    for _ in 0..3 {
        llm.push(ScriptedLlm::text(REFINED));
    }
    app.spawn_challengers(&dig.id, &id);
    wait_for(|| {
        repo::comparison::recent(&app.stores.index(), BenchTask::Escalation, 10)
            .unwrap()
            .iter()
            .any(|o| o.model_id == "challenger-a")
    })
    .await;
    let outputs = repo::comparison::recent(&app.stores.index(), BenchTask::Escalation, 10).unwrap();
    assert!(outputs.iter().all(|o| o.source == BenchSource::Shadow));
    assert!(!outputs.iter().any(|o| o.model_id == "not-enabled-b"));
    assert!(!outputs
        .iter()
        .any(|o| o.model_id == current.escalation_model));
}
