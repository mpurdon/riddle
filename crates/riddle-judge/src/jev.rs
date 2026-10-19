//! `JevJudge`: TypeSafe AI's System One endpoint behind `SemanticJudge`.
//!
//! Wire shape (from the vendor docs and the wrapper that already runs
//! in production): `POST /v1/systemone` with a bearer token and a body
//! `{model, state, questions: {id: {type, instructions, criteria?}}}`;
//! answers come back keyed by id as `{noul: p}` or `{choice, confidence,
//! probabilities}`. The model id is pinned (`jev-1.13.0`), never an alias,
//! because a cached evaluation is keyed by engine identity and an alias that
//! moves underneath would mix two models under one Mesh threshold (§26).
//!
//! The `confidence` field the vendor returns is a derived statistic that a
//! third-party audit found calibrated worse than the max probability
//! (`ARCHITECTURE-REVIEW.md` §5.1), so this module reports the distribution
//! and lets calibration (§27) decide what a threshold means.

use crate::breaker::{Admission, CircuitBreaker};
use crate::ratelimit::TokenBucket;
use crate::secret::SecretString;
use async_trait::async_trait;
use riddle_core::{
    CallLog, Distribution, JudgeAnswer, JudgeIdentity, JudgeLimits, JudgeQuestion, JudgeState,
    SemanticJudge, ServiceHealth, SignalPrimitive, UsageRow, UsageTask,
};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rdebug, rwarn, Category};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const DEFAULT_MODEL: &str = "jev-1.13.0";
pub const DEFAULT_PRICE_PER_MTOK: f64 = 0.042;

pub type UsageRecorder = Arc<dyn Fn(UsageRow) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct JevConfig {
    pub endpoint: String,
    /// Pinned model id. Aliases (`jev-latest`, `jev-preview`) are refused.
    pub model: String,
    pub timeout: Duration,
    pub requests_per_minute: u32,
    /// Consecutive failures before the breaker opens.
    pub failure_threshold: u32,
    pub cooldown: Duration,
    /// Calls in flight at once, across every caller of this client. A
    /// Sift window's streams each ask one call per Set, so the streams
    /// are not this bound; the client is.
    pub max_in_flight: usize,
    pub price_per_mtok: Option<f64>,
    /// The task usage rows are attributed to.
    pub task: UsageTask,
}

impl Default for JevConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            timeout: Duration::from_secs(5),
            requests_per_minute: 1_200,
            failure_threshold: 5,
            cooldown: Duration::from_secs(30),
            max_in_flight: JUDGE_CALLS_IN_FLIGHT,
            price_per_mtok: Some(DEFAULT_PRICE_PER_MTOK),
            task: UsageTask::Sift,
        }
    }
}

impl JevConfig {
    pub fn validate(&self) -> Result<()> {
        let m = self.model.to_ascii_lowercase();
        if m.is_empty()
            || m.ends_with("latest")
            || m.ends_with("preview")
            || !m.contains(|c: char| c.is_ascii_digit())
        {
            return Err(RiddleError::invalid(format!(
                "Jev model must be a pinned version, not {:?}",
                self.model
            )));
        }
        Ok(())
    }
}

pub struct JevJudge {
    config: JevConfig,
    key: SecretString,
    client: reqwest::Client,
    limiter: TokenBucket,
    /// The in-flight bound; the bucket paces the rate, this the count.
    in_flight: std::sync::Arc<tokio::sync::Semaphore>,
    breaker: CircuitBreaker,
    usage: Option<UsageRecorder>,
    /// When the endpoint last answered and last failed, for `health()`.
    calls: CallLog,
}

impl std::fmt::Debug for JevJudge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JevJudge")
            .field("model", &self.config.model)
            .field("breaker", &self.breaker.state())
            .finish()
    }
}

#[derive(Serialize)]
struct WireQuestion {
    #[serde(rename = "type")]
    kind: &'static str,
    instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<BTreeMap<String, String>>,
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    state: &'a str,
    questions: BTreeMap<String, WireQuestion>,
}

impl JevJudge {
    pub fn new(config: JevConfig, key: SecretString) -> Result<Self> {
        config.validate()?;
        if key.is_empty() {
            return Err(RiddleError::judge_unavailable("Jev key is empty"));
        }
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| RiddleError::internal(format!("http client: {e}")))?;
        Ok(Self {
            limiter: TokenBucket::per_minute(config.requests_per_minute),
            in_flight: std::sync::Arc::new(tokio::sync::Semaphore::new(
                config.max_in_flight.max(1),
            )),
            breaker: CircuitBreaker::new(config.failure_threshold, config.cooldown),
            client,
            key,
            config,
            usage: None,
            calls: CallLog::default(),
        })
    }

    pub async fn from_source(
        config: JevConfig,
        source: &crate::keys::JevKeySource,
    ) -> Result<Self> {
        let key = source.resolve().await?;
        Self::new(config, key)
    }

    pub fn with_usage_recorder(mut self, recorder: UsageRecorder) -> Self {
        self.usage = Some(recorder);
        self
    }

    pub fn config(&self) -> &JevConfig {
        &self.config
    }

    pub fn breaker_state(&self) -> crate::breaker::BreakerState {
        self.breaker.state()
    }

    /// Build the wire body. Public so the Raw Jev Inspector can show the
    /// request shape without a key.
    pub fn build_request<'a>(
        model: &'a str,
        state: &'a JudgeState,
        questions: &[JudgeQuestion],
    ) -> serde_json::Value {
        let mut qs = BTreeMap::new();
        for q in questions {
            let wq = match q.primitive {
                SignalPrimitive::Noul => WireQuestion {
                    kind: "noul",
                    instructions: q.definition.clone(),
                    criteria: None,
                },
                SignalPrimitive::Choice => WireQuestion {
                    kind: "choice",
                    instructions: q.definition.clone(),
                    criteria: Some(q.options.iter().cloned().collect()),
                },
            };
            qs.insert(q.id.clone(), wq);
        }
        serde_json::to_value(WireRequest {
            model,
            state: &state.text,
            questions: qs,
        })
        .unwrap_or(serde_json::Value::Null)
    }

    /// Map one answer object to a distribution over the question's options.
    pub fn parse_answer(
        q: &JudgeQuestion,
        answer: &serde_json::Value,
    ) -> Option<(String, Distribution)> {
        match q.primitive {
            SignalPrimitive::Noul => {
                let p = answer
                    .get("noul")
                    .and_then(|v| v.as_f64())
                    .or_else(|| answer.get("probability").and_then(|v| v.as_f64()))?;
                let p = p.clamp(0.0, 1.0);
                let mut d = Distribution::new();
                d.insert("YES".into(), p);
                d.insert("NO".into(), 1.0 - p);
                Some((if p >= 0.5 { "YES".into() } else { "NO".into() }, d))
            }
            SignalPrimitive::Choice => {
                let choice = answer.get("choice").and_then(|v| v.as_str())?.to_string();
                let mut d = Distribution::new();
                if let Some(probs) = answer.get("probabilities").and_then(|v| v.as_object()) {
                    for (name, _) in &q.options {
                        d.insert(
                            name.clone(),
                            probs.get(name).and_then(|v| v.as_f64()).unwrap_or(0.0),
                        );
                    }
                } else {
                    let c = answer
                        .get("confidence")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    let others = q.options.len().saturating_sub(1).max(1) as f64;
                    for (name, _) in &q.options {
                        d.insert(
                            name.clone(),
                            if *name == choice {
                                c
                            } else {
                                (1.0 - c) / others
                            },
                        );
                    }
                }
                Some((choice, d))
            }
        }
    }

    fn record_usage(&self, input_tokens: u64, latency_ms: u64, ok: bool) {
        if let Some(r) = &self.usage {
            let cost = if ok {
                self.config
                    .price_per_mtok
                    .map(|p| input_tokens as f64 * p / 1_000_000.0)
            } else {
                Some(0.0)
            };
            r(UsageRow {
                id: uuid::Uuid::now_v7().to_string(),
                at: chrono::Utc::now(),
                provider: "jev".into(),
                model: self.config.model.clone(),
                task: self.config.task,
                dig_id: None,
                trace_id: None,
                input_tokens,
                output_tokens: 0,
                cost_usd: cost,
                latency_ms,
                ok,
            });
        }
    }

    /// A turn at the in-flight bound; held across one request.
    async fn slot(&self) -> Result<tokio::sync::OwnedSemaphorePermit> {
        self.in_flight
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| RiddleError::judge_unavailable("judge slots closed"))
    }

    async fn post(&self, body: &serde_json::Value) -> Result<(serde_json::Value, u64)> {
        let started = Instant::now();
        let resp = self
            .client
            .post(&self.config.endpoint)
            .bearer_auth(self.key.expose_secret())
            .json(body)
            .send()
            .await
            .map_err(|e| {
                let why = if e.is_timeout() {
                    "timeout"
                } else if e.is_connect() {
                    "connect failure"
                } else {
                    "request failure"
                };
                RiddleError::judge_unavailable(format!("Jev {why}"))
            })?;
        let status = resp.status();
        let latency = started.elapsed().as_millis() as u64;
        if !status.is_success() {
            let code = status.as_u16();
            let retry_after_secs = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok());
            let kind = match code {
                429 => "rate limited",
                401 | 403 => "authentication rejected",
                400 => "request rejected",
                500..=599 => "server error",
                _ => "unexpected status",
            };
            rwarn!(
                Category::Judge,
                "Jev HTTP {code} ({kind}) after {latency}ms"
            );
            return Err(
                RiddleError::judge_unavailable(format!("Jev HTTP {code}: {kind}")).with_detail(
                    serde_json::json!({
                        "status": code,
                        "latencyMs": latency,
                        "retryable": matches!(code, 429 | 500..=599),
                        "retryAfterSecs": retry_after_secs,
                    }),
                ),
            );
        }
        let value: serde_json::Value = resp.json().await.map_err(|_| {
            RiddleError::judge_unavailable("Jev returned a malformed response")
                .with_detail(serde_json::json!({ "retryable": false }))
        })?;
        Ok((value, latency))
    }
}

fn usage_tokens(v: &serde_json::Value) -> Option<u64> {
    let u = v.get("usage")?;
    [
        "input_tokens",
        "prompt_tokens",
        "state_tokens",
        "tokens",
        "total_tokens",
    ]
    .iter()
    .find_map(|k| u.get(k).and_then(|x| x.as_u64()))
}

/// Calls one Jev client carries at once: six Sift streams with two Sets
/// each, fully overlapped. The bucket of 1,200 calls a minute, at about
/// 400 ms a call, is saturated by eight.
pub const JUDGE_CALLS_IN_FLIGHT: usize = 12;

/// The pause before the one retry of a transient failure, and the most
/// Retry-After may stretch it.
const RETRY_DELAY: Duration = Duration::from_millis(500);
const MAX_RETRY_WAIT: Duration = Duration::from_secs(5);

struct ProbeGuard<'a> {
    breaker: &'a CircuitBreaker,
    resolved: bool,
}

impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        if !self.resolved {
            self.breaker.abandon_probe();
        }
    }
}

#[async_trait]
impl SemanticJudge for JevJudge {
    async fn evaluate(
        &self,
        state: &JudgeState,
        questions: &[JudgeQuestion],
    ) -> Result<Vec<JudgeAnswer>> {
        if questions.is_empty() {
            return Ok(vec![]);
        }
        let limits = self.limits();
        if state.approx_tokens > limits.max_state_tokens {
            return Err(RiddleError::invalid(format!(
                "state of ~{} tokens exceeds the judge limit of {}",
                state.approx_tokens, limits.max_state_tokens
            )));
        }
        let admission = self.breaker.admit();
        if admission == Admission::Reject {
            return Err(RiddleError::judge_unavailable("Jev circuit is open")
                .with_detail(self.breaker.state()));
        }
        let mut guard = ProbeGuard {
            breaker: &self.breaker,
            resolved: false,
        };
        let body = Self::build_request(&self.config.model, state, questions);
        let estimate = state.approx_tokens as u64
            + questions
                .iter()
                .map(|q| (q.definition.len() / 4) as u64)
                .sum::<u64>();
        // One bounded retry on a transient failure before the breaker hears
        // of it: a single timeout stranded a message as Unresolved with
        // nothing to decide it again until a rule changed (19 Oct 2026). The
        // wait honours Retry-After, capped so a window never waits long on
        // one message, and the in-flight slot is given back for the wait so
        // a call that is ready takes it.
        let mut slot = self.slot().await?;
        self.limiter.acquire().await;
        let result = match self.post(&body).await {
            Err(e) if e.is_retryable() => {
                drop(slot);
                let wait = e.retry_after().unwrap_or(RETRY_DELAY).min(MAX_RETRY_WAIT);
                rdebug!(
                    Category::Judge,
                    "{}; retrying once after {}ms",
                    e.message,
                    wait.as_millis()
                );
                tokio::time::sleep(wait).await;
                slot = self.slot().await?;
                self.limiter.acquire().await;
                self.post(&body).await
            }
            r => r,
        };
        drop(slot);
        match result {
            Ok((value, latency)) => {
                let Some(answers) = value.get("answers").and_then(|a| a.as_object()) else {
                    self.breaker.record_failure();
                    guard.resolved = true;
                    self.record_usage(estimate, latency, false);
                    let e = RiddleError::judge_unavailable("Jev response has no answers")
                        .with_detail(serde_json::json!({ "retryable": false }));
                    self.calls.failed(chrono::Utc::now(), e.kind.as_str());
                    return Err(e);
                };
                let input_tokens = usage_tokens(&value).unwrap_or(estimate);
                let mut out = Vec::with_capacity(questions.len());
                for q in questions {
                    let Some(a) = answers.get(&q.id) else {
                        rwarn!(
                            Category::Judge,
                            "Jev omitted an answer for question {}",
                            q.id
                        );
                        continue;
                    };
                    let Some((selected, distribution)) = Self::parse_answer(q, a) else {
                        rwarn!(
                            Category::Judge,
                            "Jev answer for {} has an unexpected shape",
                            q.id
                        );
                        continue;
                    };
                    out.push(JudgeAnswer {
                        question_id: q.id.clone(),
                        selected,
                        distribution,
                        latency_ms: latency,
                        input_tokens,
                        deterministic: false,
                        raw: Some(value.clone()),
                    });
                }
                self.breaker.record_success();
                guard.resolved = true;
                self.calls.succeeded(chrono::Utc::now());
                self.record_usage(input_tokens, latency, true);
                rdebug!(
                    Category::Judge,
                    "Jev answered {} of {} questions in {latency}ms, {input_tokens} tokens",
                    out.len(),
                    questions.len()
                );
                Ok(out)
            }
            Err(e) => {
                self.breaker.record_failure();
                guard.resolved = true;
                self.calls.failed(chrono::Utc::now(), e.kind.as_str());
                self.record_usage(estimate, self.config.timeout.as_millis() as u64, false);
                Err(e)
            }
        }
    }

    fn limits(&self) -> JudgeLimits {
        JudgeLimits {
            max_state_tokens: 32_000,
            max_request_tokens: 64_000,
            max_questions: 64,
            requests_per_minute: self.config.requests_per_minute,
        }
    }

    fn identity(&self) -> JudgeIdentity {
        JudgeIdentity {
            provider: "jev".into(),
            model_id: self.config.model.clone(),
            model_version: self.config.model.clone(),
            price_per_mtok: self.config.price_per_mtok,
        }
    }

    fn health(&self) -> ServiceHealth {
        self.calls.health(self.breaker.health())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::JevKeySource;

    fn noul(id: &str, def: &str) -> JudgeQuestion {
        JudgeQuestion {
            id: id.into(),
            primitive: SignalPrimitive::Noul,
            definition: def.into(),
            options: vec![("YES".into(), "yes".into()), ("NO".into(), "no".into())],
        }
    }

    #[test]
    fn the_request_body_matches_the_system_one_wire_shape() {
        let state = JudgeState::new("hello".into());
        let choice = JudgeQuestion {
            id: "kind@v1".into(),
            primitive: SignalPrimitive::Choice,
            definition: "What kind?".into(),
            options: vec![("A".into(), "an a".into()), ("B".into(), "a b".into())],
        };
        let body = JevJudge::build_request(
            "jev-1.13.0",
            &state,
            &[
                noul("asks_question@v1", "The text asks a question."),
                choice,
            ],
        );
        assert_eq!(body["model"], "jev-1.13.0");
        assert_eq!(body["state"], "hello");
        assert_eq!(body["questions"]["asks_question@v1"]["type"], "noul");
        assert!(body["questions"]["asks_question@v1"]
            .get("criteria")
            .is_none());
        assert_eq!(body["questions"]["kind@v1"]["type"], "choice");
        assert_eq!(body["questions"]["kind@v1"]["criteria"]["A"], "an a");
    }

    #[test]
    fn answers_map_to_distributions_over_option_names() {
        let q = noul("x@v1", "claim");
        let (sel, d) = JevJudge::parse_answer(&q, &serde_json::json!({ "noul": 0.93 })).unwrap();
        assert_eq!(sel, "YES");
        assert!((d["YES"] - 0.93).abs() < 1e-9 && (d["NO"] - 0.07).abs() < 1e-9);
        let c = JudgeQuestion {
            id: "k@v1".into(),
            primitive: SignalPrimitive::Choice,
            definition: "d".into(),
            options: vec![
                ("A".into(), "a".into()),
                ("B".into(), "b".into()),
                ("C".into(), "c".into()),
            ],
        };
        let (sel, d) = JevJudge::parse_answer(&c, &serde_json::json!({ "choice": "B", "confidence": 0.7, "probabilities": { "A": 0.1, "B": 0.8, "C": 0.1 } })).unwrap();
        assert_eq!(sel, "B");
        assert_eq!(d["B"], 0.8);
        let (_, d) =
            JevJudge::parse_answer(&c, &serde_json::json!({ "choice": "B", "confidence": 0.7 }))
                .unwrap();
        assert!((d["B"] - 0.7).abs() < 1e-9 && (d["A"] - 0.15).abs() < 1e-9);
    }

    #[test]
    fn an_alias_model_id_is_refused_and_debug_output_hides_the_key() {
        let bad = JevConfig {
            model: "jev-latest".into(),
            ..Default::default()
        };
        assert!(JevJudge::new(bad, SecretString::new("k")).is_err());
        let judge = JevJudge::new(JevConfig::default(), SecretString::new("sk-hidden")).unwrap();
        assert!(!format!("{judge:?}").contains("hidden"));
    }

    #[tokio::test]
    async fn an_open_breaker_answers_unavailable_without_a_network_call() {
        let judge = JevJudge::new(
            JevConfig {
                failure_threshold: 1,
                cooldown: Duration::from_secs(600),
                endpoint: "http://127.0.0.1:9/systemone".into(),
                timeout: Duration::from_millis(200),
                ..Default::default()
            },
            SecretString::new("k"),
        )
        .unwrap();
        let state = JudgeState::new("x".into());
        let err = judge
            .evaluate(&state, &[noul("a@v1", "claim")])
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::JudgeUnavailable);
        assert!(judge.breaker.is_open());
        let started = Instant::now();
        let err = judge
            .evaluate(&state, &[noul("a@v1", "claim")])
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::JudgeUnavailable);
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "open breaker must answer immediately"
        );
    }

    /// Proves: the Jev key resolves from SSM (the profile and parameter in
    /// `RIDDLE_DEV_AWS_PROFILE` and `RIDDLE_DEV_JEV_SSM_PARAMETER`) and one real System One request with a
    /// noul and a choice question returns answers of the expected shape:
    /// one answer per question, distributions over the option names summing
    /// to one, tokens and latency recorded. Asserts on shape only; never
    /// prints the key or the response.
    /// Run: `cargo test -p riddle-judge --release -- --ignored live_`
    #[tokio::test]
    #[ignore]
    async fn live_jev_resolves_the_key_from_ssm_and_answers_a_noul_and_a_choice_question() {
        let judge = JevJudge::from_source(
            JevConfig::default(),
            &JevKeySource::dev_ssm().expect(
                "RIDDLE_DEV_AWS_PROFILE and RIDDLE_DEV_JEV_SSM_PARAMETER name the owner's key",
            ),
        )
        .await
        .expect("key resolution and client construction");
        let state = JudgeState::new("Subject: Review requested: Add retry to the sync worker\nFrom: notifications@github.com\n\n@you requested your review on #482. CI passed.".into());
        let choice = JudgeQuestion {
            id: "kind@v1".into(),
            primitive: SignalPrimitive::Choice,
            definition: "What kind of message is this?".into(),
            options: vec![
                (
                    "review".into(),
                    "Someone asks the recipient to review a change.".into(),
                ),
                (
                    "failure".into(),
                    "An automated check reports a failure.".into(),
                ),
                ("other".into(), "Anything else.".into()),
            ],
        };
        let started = Instant::now();
        let answers = judge.evaluate(&state, &[noul("review_requested@v1", "Someone explicitly asks the recipient to review a change, document or submission."), choice]).await.expect("live evaluation");
        let latency = started.elapsed().as_millis();
        assert_eq!(answers.len(), 2);
        for a in &answers {
            let sum: f64 = a.distribution.values().sum();
            assert!((sum - 1.0).abs() < 0.02, "distribution sums to {sum}");
            assert!(a.input_tokens > 0);
            assert!(!a.deterministic);
        }
        let review = answers
            .iter()
            .find(|a| a.question_id == "review_requested@v1")
            .unwrap();
        assert!(review.distribution.contains_key("YES"));
        eprintln!(
            "live Jev: {} answers in {latency}ms, ~{} input tokens",
            answers.len(),
            answers[0].input_tokens
        );
    }
}
