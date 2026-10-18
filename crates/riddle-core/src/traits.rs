//! Cross-domain interfaces. Sift depends on these, not on a vendor (§18, §39).

use crate::signal::{Distribution, Signal, SignalEvaluation, SignalPrimitive};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use riddle_errors::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A dependency's circuit breaker (§56) as the Task Centre reads it. The
/// judge's breaker opened and closed for a fortnight and nothing read its
/// state: every Dig said "Sifting" while every call was refused (17 Oct
/// 2026). `Unknown` is the doubles' answer, which keep no breaker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum BreakerHealth {
    #[default]
    Unknown,
    Closed {
        consecutive_failures: u32,
    },
    /// Every call is refused until `until`.
    Open {
        until: DateTime<Utc>,
    },
    /// The cooldown has passed; the next call is the one probe.
    HalfOpen,
}

/// What a judge or LLM client says about itself without making a call: its
/// breaker, and when it last succeeded and last failed. The failure kind is
/// `ErrorKind::as_str`, never the error's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServiceHealth {
    pub breaker: BreakerHealth,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub last_failure_kind: Option<String>,
}

impl ServiceHealth {
    /// The later of the last success and the last failure.
    pub fn last_call_at(&self) -> Option<DateTime<Utc>> {
        self.last_success_at.max(self.last_failure_at)
    }
}

#[derive(Default)]
struct CallLogInner {
    success_at: Option<DateTime<Utc>>,
    failure_at: Option<DateTime<Utc>>,
    failure_kind: Option<String>,
}

/// The last-call bookkeeping behind `ServiceHealth`, shared by the judge
/// and the LLM client so both answer the same questions the same way.
#[derive(Default)]
pub struct CallLog(parking_lot_lite::Mutex<CallLogInner>);

impl CallLog {
    pub fn succeeded(&self, at: DateTime<Utc>) {
        self.0.lock().success_at = Some(at);
    }
    pub fn failed(&self, at: DateTime<Utc>, kind: &str) {
        let mut g = self.0.lock();
        g.failure_at = Some(at);
        g.failure_kind = Some(kind.to_string());
    }
    pub fn health(&self, breaker: BreakerHealth) -> ServiceHealth {
        let g = self.0.lock();
        ServiceHealth {
            breaker,
            last_success_at: g.success_at,
            last_failure_at: g.failure_at,
            last_failure_kind: g.failure_kind.clone(),
        }
    }
}

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
        Self {
            text,
            approx_tokens,
        }
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
            options: s
                .choices
                .iter()
                .map(|c| (c.name.clone(), c.definition.clone()))
                .collect(),
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
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.model_id)
    }
}

/// The semantic judgment engine (§18).
#[async_trait]
pub trait SemanticJudge: Send + Sync {
    async fn evaluate(
        &self,
        state: &JudgeState,
        questions: &[JudgeQuestion],
    ) -> Result<Vec<JudgeAnswer>>;
    fn limits(&self) -> JudgeLimits;
    fn identity(&self) -> JudgeIdentity;
    /// The breaker and the last calls, without making one. The doubles keep
    /// neither and answer `Unknown`.
    fn health(&self) -> ServiceHealth {
        ServiceHealth::default()
    }
    /// A cheap liveness probe for the Developer pane and the circuit breaker.
    async fn probe(&self) -> Result<u64> {
        let s = JudgeState::new("The meeting is at noon.".into());
        let q = JudgeQuestion {
            id: "probe".into(),
            primitive: SignalPrimitive::Noul,
            definition: "The text mentions a time of day.".into(),
            options: vec![
                ("YES".into(), "It does.".into()),
                ("NO".into(), "It does not.".into()),
            ],
        };
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
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

/// A clock that tests advance by hand.
pub struct FixedClock(pub parking_lot_lite::Mutex<chrono::DateTime<chrono::Utc>>);
impl FixedClock {
    pub fn at(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(parking_lot_lite::Mutex::new(t))
    }
    pub fn advance(&self, d: chrono::Duration) {
        let mut g = self.0.lock();
        *g += d;
    }
}
impl Clock for FixedClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        *self.0.lock()
    }
}

/// A tiny mutex wrapper so core does not pull in parking_lot for one field.
pub mod parking_lot_lite {
    pub struct Mutex<T>(std::sync::Mutex<T>);
    impl<T: Default> Default for Mutex<T> {
        fn default() -> Self {
            Self(std::sync::Mutex::new(T::default()))
        }
    }
    impl<T> Mutex<T> {
        pub fn new(t: T) -> Self {
            Self(std::sync::Mutex::new(t))
        }
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }
    }
}

/// Where domain code publishes events for the UI (§7). Emitting never fails
/// the work: a sink that cannot deliver drops the event.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: crate::api::RiddleEvent);
}

pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _event: crate::api::RiddleEvent) {}
}

/// A sink that keeps events for tests.
#[derive(Default)]
pub struct RecordingSink(pub parking_lot_lite::Mutex<Vec<crate::api::RiddleEvent>>);
impl EventSink for RecordingSink {
    fn emit(&self, event: crate::api::RiddleEvent) {
        self.0.lock().push(event);
    }
}
impl RecordingSink {
    pub fn take(&self) -> Vec<crate::api::RiddleEvent> {
        std::mem::take(&mut *self.0.lock())
    }
}

/// Where Sift and the correction Actions hand a classification to the
/// provider projection outbox (§15). Track A's outbox implements it; Track B
/// calls it when present so the two meet at integration. Projection is
/// bookkeeping from Sift's point of view: a failure is reported, never a
/// reason to lose the classification.
pub trait ProjectionHook: Send + Sync {
    fn project(
        &self,
        dig: &crate::ids::DigId,
        classification: &crate::classification::Classification,
    ) -> Result<()>;
    /// `project` for a caller that already holds the message: Sift persists
    /// the decision of a message it has just read, and the outbox hook read
    /// it again, bodies and all, for every decision (17 Oct 2026). The
    /// default ignores the message and calls `project`.
    fn project_message(
        &self,
        dig: &crate::ids::DigId,
        message: &crate::mail::Message,
        classification: &crate::classification::Classification,
    ) -> Result<()> {
        let _ = message;
        self.project(dig, classification)
    }
    fn unproject(&self, dig: &crate::ids::DigId, message_id: &crate::ids::MessageId) -> Result<()>;
}

/// Sized handle so a `dyn ProjectionHook` can be registered in a typed
/// service map (`Services::insert(Arc::new(ProjectionHookHandle(hook)))`).
pub struct ProjectionHookHandle(pub std::sync::Arc<dyn ProjectionHook>);
