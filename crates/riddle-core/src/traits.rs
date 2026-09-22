//! Cross-domain interfaces. Sift depends on these, not on a vendor (§18, §39).

use crate::signal::{Distribution, Signal, SignalEvaluation, SignalPrimitive};
use async_trait::async_trait;
use riddle_errors::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Size-bounded state given to the judge once per Signal Set (§25).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeState {
    pub text: String,
    /// Rough token estimate, so the state builder can respect the budget.
    pub approx_tokens: u32,
}

impl JudgeState {
    pub fn new(text: String) -> Self {
        let approx_tokens = (text.len() / 4) as u32;
        Self { text, approx_tokens }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeQuestion {
    pub id: String,
    pub primitive: SignalPrimitive,
    pub definition: String,
    /// (name, definition). For noul: YES/NO.
    pub options: Vec<(String, String)>,
}

impl JudgeQuestion {
    pub fn from_signal(s: &Signal) -> Self {
        Self {
            id: s.key(),
            primitive: s.primitive,
            definition: s.definition.clone(),
            options: s.choices.iter().map(|c| (c.name.clone(), c.definition.clone())).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JudgeAnswer {
    pub question_id: String,
    pub selected: String,
    pub distribution: Distribution,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub deterministic: bool,
    /// The engine's raw response, for the Raw Jev Inspector (§69).
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JudgeLimits {
    pub max_state_tokens: u32,
    pub max_request_tokens: u32,
    pub max_questions: u32,
    pub requests_per_minute: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JudgeIdentity {
    pub provider: String,
    pub model_id: String,
    pub model_version: String,
    /// USD per million input tokens; `None` means unpriced.
    pub price_per_mtok: Option<f64>,
}

impl JudgeIdentity {
    pub fn key(&self) -> String { format!("{}:{}", self.provider, self.model_id) }
}

/// The semantic judgment engine (§18).
#[async_trait]
pub trait SemanticJudge: Send + Sync {
    async fn evaluate(&self, state: &JudgeState, questions: &[JudgeQuestion]) -> Result<Vec<JudgeAnswer>>;
    fn limits(&self) -> JudgeLimits;
    fn identity(&self) -> JudgeIdentity;
    /// A cheap liveness probe for the Developer pane and the circuit breaker.
    async fn probe(&self) -> Result<u64> {
        let s = JudgeState::new("The meeting is at noon.".into());
        let q = JudgeQuestion { id: "probe".into(), primitive: SignalPrimitive::Noul, definition: "The text mentions a time of day.".into(), options: vec![("YES".into(), "It does.".into()), ("NO".into(), "It does not.".into())] };
        let started = std::time::Instant::now();
        self.evaluate(&s, &[q]).await?;
        Ok(started.elapsed().as_millis() as u64)
    }
}

/// What the escalation LLM receives (§39).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefineRequest {
    pub message_text: String,
    pub thread_state: Option<String>,
    pub context: Vec<String>,
    pub signals: Vec<SignalEvaluation>,
    pub definitions: Vec<Signal>,
    pub reason: String,
    pub language: String,
}

/// Refined Signals with a rationale. Never a disposition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefineResponse {
    pub refined: Vec<SignalEvaluation>,
    pub rationale: String,
    pub model_id: String,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
}

#[async_trait]
pub trait SignalRefiner: Send + Sync {
    async fn refine(&self, req: RefineRequest) -> Result<RefineResponse>;
    fn identity(&self) -> String;
}

/// Injectable clock so Temporal Sift and Shadow Period tests do not sleep.
pub trait Clock: Send + Sync {
    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> { chrono::Utc::now() }
}

/// A clock that tests advance by hand.
pub struct FixedClock(pub parking_lot_lite::Mutex<chrono::DateTime<chrono::Utc>>);
impl FixedClock {
    pub fn at(t: chrono::DateTime<chrono::Utc>) -> Self { Self(parking_lot_lite::Mutex::new(t)) }
    pub fn advance(&self, d: chrono::Duration) { let mut g = self.0.lock(); *g += d; }
}
impl Clock for FixedClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> { *self.0.lock() }
}

/// A tiny mutex wrapper so core does not pull in parking_lot for one field.
pub mod parking_lot_lite {
    pub struct Mutex<T>(std::sync::Mutex<T>);
    impl<T> Mutex<T> {
        pub fn new(t: T) -> Self { Self(std::sync::Mutex::new(t)) }
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> { self.0.lock().unwrap_or_else(|e| e.into_inner()) }
    }
}
