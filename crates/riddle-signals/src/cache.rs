//! The semantic Signal cache (§26): read what is cached, evaluate only the
//! misses in one judge call, apply calibration, write back. The Activity
//! note "13 matched on cached Signals, 1 needed a fresh evaluation" is the
//! `cached`/`fresh` pair here. The cache key is (message, name, version,
//! engine identity, context hash); a message-only Signal uses an empty
//! context hash and is therefore cached for good.

use riddle_core::{
    sha256_hex, Distribution, JudgeQuestion, JudgeState, MessageId, SemanticJudge, Signal,
    SignalEvaluation, SignalPrimitive,
};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rdebug, rwarn, Category};
use riddle_store::{repo, Store};

#[derive(Debug, Clone, Default)]
pub struct CacheOutcome {
    /// One per requested Signal, in the requested order.
    pub evaluations: Vec<SignalEvaluation>,
    pub cached: usize,
    pub fresh: usize,
    pub judge_ms: u64,
    /// Input tokens of the one judge request made, or zero.
    pub input_tokens: u64,
    pub cost_usd: Option<f64>,
    pub questions: u32,
    pub request_hash: Option<String>,
    /// The engine's raw response for the Raw Jev Inspector (§69).
    pub raw: Option<serde_json::Value>,
}

/// Build the question the judge sees for a Signal. The id is the Signal
/// key so answers map back without a lookup table.
pub fn question_for(signal: &Signal) -> JudgeQuestion {
    JudgeQuestion::from_signal(signal)
}

/// Turn a judge distribution into an evaluation, applying the Signal's
/// calibration. Noul: YES is calibrated and NO follows; Choice: each entry is
/// mapped and the distribution renormalized.
#[allow(clippy::too_many_arguments)]
pub fn evaluation_from(
    signal: &Signal,
    message_id: &MessageId,
    engine_identity: &str,
    distribution: Distribution,
    latency_ms: u64,
    input_tokens: u64,
    deterministic: bool,
    context_hash: Option<String>,
    request_hash: Option<String>,
    cached: bool,
) -> SignalEvaluation {
    let calibrated = calibrate(signal, &distribution);
    let (selected, confidence) = argmax(&calibrated);
    SignalEvaluation {
        message_id: message_id.clone(),
        signal_name: signal.name.clone(),
        signal_version: signal.version,
        engine_identity: engine_identity.to_string(),
        selected,
        distribution,
        calibrated,
        confidence,
        latency_ms,
        input_tokens,
        cached,
        deterministic,
        context_hash,
        request_hash,
        evaluated_at: chrono::Utc::now(),
    }
}

fn calibrate(signal: &Signal, raw: &Distribution) -> Distribution {
    let Some(cal) = &signal.calibration else {
        return raw.clone();
    };
    match signal.primitive {
        SignalPrimitive::Noul => {
            let yes = cal.apply(raw.get("YES").copied().unwrap_or(0.0));
            let mut d = Distribution::new();
            d.insert("YES".into(), yes);
            d.insert("NO".into(), 1.0 - yes);
            d
        }
        SignalPrimitive::Choice => {
            let mut d: Distribution = raw
                .iter()
                .map(|(k, v)| (k.clone(), cal.apply(*v)))
                .collect();
            let total: f64 = d.values().sum();
            if total > 0.0 {
                for v in d.values_mut() {
                    *v /= total;
                }
            }
            d
        }
    }
}

fn argmax(d: &Distribution) -> (String, f64) {
    d.iter().fold((String::new(), -1.0), |best, (k, v)| {
        if *v > best.1 {
            (k.clone(), *v)
        } else {
            best
        }
    })
}

/// Read cached evaluations, evaluate the misses in one call, write back.
/// `context_hash` is the hash of the Context the state carried; it keys only
/// Context-dependent Signals. With `write_back` false the result is
/// experimental and never enters the cache (invariant 17).
pub async fn cached_or_evaluate(
    store: &Store,
    judge: &dyn SemanticJudge,
    message_id: &MessageId,
    signals: &[Signal],
    state: &JudgeState,
    context_hash: &str,
    write_back: bool,
) -> Result<CacheOutcome> {
    let engine = judge.identity().key();
    let price = judge.identity().price_per_mtok;
    let mut out = CacheOutcome::default();
    let mut slots: Vec<Option<SignalEvaluation>> = Vec::with_capacity(signals.len());
    let mut misses: Vec<usize> = Vec::new();
    // One read for everything this engine has evaluated on the message,
    // then a lookup per Signal; it was one read per Signal, and the fresh
    // answers below go back in one write (17 Oct 2026). The key is the
    // same: name, version, engine, and the context hash for a
    // Context-dependent Signal (an empty hash otherwise).
    let known = repo::signals::evaluations_for_engine(store, message_id, &engine)?;
    for (i, s) in signals.iter().enumerate() {
        let key_hash = if s.inputs.is_context_dependent() {
            context_hash
        } else {
            ""
        };
        let hit = known.iter().find(|e| {
            e.signal_name == s.name
                && e.signal_version == s.version
                && e.context_hash.as_deref().unwrap_or("") == key_hash
        });
        match hit {
            Some(e) => {
                let mut e = e.clone();
                e.cached = true;
                slots.push(Some(e));
                out.cached += 1;
            }
            None => {
                slots.push(None);
                misses.push(i);
            }
        }
    }
    if !misses.is_empty() {
        let questions: Vec<JudgeQuestion> =
            misses.iter().map(|&i| question_for(&signals[i])).collect();
        let limits = judge.limits();
        if questions.len() as u32 > limits.max_questions {
            return Err(RiddleError::invalid(format!(
                "{} questions exceed the judge limit of {}",
                questions.len(),
                limits.max_questions
            )));
        }
        let mut hash_input = state.text.clone();
        for q in &questions {
            hash_input.push_str(&q.id);
            hash_input.push_str(&q.definition);
        }
        let request_hash = sha256_hex(hash_input.as_bytes());
        let started = std::time::Instant::now();
        let answers = judge.evaluate(state, &questions).await?;
        out.judge_ms = started.elapsed().as_millis() as u64;
        out.questions = questions.len() as u32;
        out.request_hash = Some(request_hash.clone());
        out.input_tokens = answers.iter().map(|a| a.input_tokens).max().unwrap_or(0);
        out.cost_usd = price.map(|p| out.input_tokens as f64 * p / 1_000_000.0);
        out.raw = answers.iter().find_map(|a| a.raw.clone());
        let mut fresh: Vec<SignalEvaluation> = Vec::with_capacity(misses.len());
        for &i in &misses {
            let s = &signals[i];
            let Some(a) = answers.iter().find(|a| a.question_id == s.key()) else {
                rwarn!(
                    Category::Judge,
                    "judge returned no answer for signal {}",
                    s.key()
                );
                continue;
            };
            let key_hash = if s.inputs.is_context_dependent() {
                Some(context_hash.to_string())
            } else {
                None
            };
            let e = evaluation_from(
                s,
                message_id,
                &engine,
                a.distribution.clone(),
                a.latency_ms,
                a.input_tokens,
                a.deterministic,
                key_hash,
                Some(request_hash.clone()),
                false,
            );
            slots[i] = Some(e.clone());
            fresh.push(e);
            out.fresh += 1;
        }
        if write_back && !fresh.is_empty() {
            let put = store.write(|c| {
                for e in &fresh {
                    repo::signals::put_evaluation_in(c, e)?;
                }
                Ok(())
            });
            if let Err(err) = put {
                // Bookkeeping never fails the work: the evaluations are still used.
                riddle_observability::swallowed!(
                    Category::Judge,
                    "signal_cache_write",
                    err,
                    "could not cache {} evaluations for message {}",
                    fresh.len(),
                    message_id
                );
            }
        }
    }
    out.evaluations = slots.into_iter().flatten().collect();
    rdebug!(
        Category::Judge,
        "signals for message {}: {} cached, {} fresh",
        message_id,
        out.cached,
        out.fresh
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::system_signal;
    use riddle_judge::ScriptedJudge;
    use riddle_store::StoreKind;

    #[tokio::test]
    async fn misses_are_evaluated_once_and_then_served_from_the_cache() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let judge = ScriptedJudge::default();
        let signals = vec![
            system_signal("review_requested").unwrap(),
            system_signal("routine").unwrap(),
        ];
        let id = MessageId::for_provider(&riddle_core::DigId("d".into()), "m1");
        let state = JudgeState::new("Subject: review requested on #12".into());
        let first = cached_or_evaluate(&store, &judge, &id, &signals, &state, "", true)
            .await
            .unwrap();
        assert_eq!((first.cached, first.fresh), (0, 2));
        assert_eq!(first.questions, 2);
        assert!(first.evaluations[0].score(None) > 0.9);
        let second = cached_or_evaluate(&store, &judge, &id, &signals, &state, "", true)
            .await
            .unwrap();
        assert_eq!((second.cached, second.fresh), (2, 0));
        assert!(second.evaluations.iter().all(|e| e.cached));
        assert_eq!(judge.calls(), 1);
    }

    #[tokio::test]
    async fn one_read_serves_a_mix_of_hits_and_misses_in_the_requested_order_with_the_same_scores()
    {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let judge = ScriptedJudge::default();
        let review = system_signal("review_requested").unwrap();
        let routine = system_signal("routine").unwrap();
        let responds = system_signal("requires_response").unwrap();
        let id = MessageId::for_provider(&riddle_core::DigId("d".into()), "m1");
        let state = JudgeState::new("Subject: review requested on #12".into());
        let first = cached_or_evaluate(
            &store,
            &judge,
            &id,
            std::slice::from_ref(&review),
            &state,
            "ctx",
            true,
        )
        .await
        .unwrap();
        assert_eq!((first.cached, first.fresh), (0, 1));
        // One hit between two misses: the order and the hit's scores hold.
        let mixed = cached_or_evaluate(
            &store,
            &judge,
            &id,
            &[routine.clone(), review.clone(), responds.clone()],
            &state,
            "ctx",
            true,
        )
        .await
        .unwrap();
        assert_eq!((mixed.cached, mixed.fresh), (1, 2));
        assert_eq!(
            mixed
                .evaluations
                .iter()
                .map(|e| (e.signal_name.as_str(), e.cached))
                .collect::<Vec<_>>(),
            vec![
                ("routine", false),
                ("review_requested", true),
                ("requires_response", false)
            ]
        );
        let hit = &mixed.evaluations[1];
        assert_eq!(hit.selected, first.evaluations[0].selected);
        // Scores round-trip through the JSON column, so within a ULP.
        assert!((hit.confidence - first.evaluations[0].confidence).abs() < 1e-9);
        assert!(
            (hit.score(None) - first.evaluations[0].score(None)).abs() < 1e-9,
            "{} vs {}",
            hit.score(None),
            first.evaluations[0].score(None)
        );
        assert_eq!(hit.request_hash, first.evaluations[0].request_hash);
        // The two misses went back in one write: everything is a hit now.
        let all = cached_or_evaluate(
            &store,
            &judge,
            &id,
            &[routine, review, responds],
            &state,
            "ctx",
            true,
        )
        .await
        .unwrap();
        assert_eq!((all.cached, all.fresh), (3, 0));
        assert_eq!(
            all.evaluations
                .iter()
                .map(|e| e.selected.clone())
                .collect::<Vec<_>>(),
            mixed
                .evaluations
                .iter()
                .map(|e| e.selected.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(judge.calls(), 2);
        assert_eq!(
            repo::signals::evaluations_for(&store, &id).unwrap().len(),
            3
        );
    }

    #[tokio::test]
    async fn a_context_dependent_signal_is_keyed_by_the_context_hash_and_experiments_never_write() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let judge = ScriptedJudge::default();
        let signals = vec![system_signal("requires_response").unwrap()];
        let id = MessageId::for_provider(&riddle_core::DigId("d".into()), "m1");
        let state = JudgeState::new("Can you send the file?".into());
        cached_or_evaluate(&store, &judge, &id, &signals, &state, "ctx-a", true)
            .await
            .unwrap();
        let again = cached_or_evaluate(&store, &judge, &id, &signals, &state, "ctx-b", true)
            .await
            .unwrap();
        assert_eq!(again.fresh, 1);
        let experimental =
            cached_or_evaluate(&store, &judge, &id, &signals, &state, "ctx-c", false)
                .await
                .unwrap();
        assert_eq!(experimental.fresh, 1);
        assert_eq!(
            repo::signals::evaluations_for(&store, &id).unwrap().len(),
            2
        );
    }
}
