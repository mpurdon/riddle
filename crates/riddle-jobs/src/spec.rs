//! What a job is made of beyond `riddle_core::Job`: the enqueue request, the
//! checkpoint envelope, pause reasons and the per-kind facts the scheduler
//! needs (resource class, long-running).
//!
//! The payload lives inside `Job.checkpoint` as `{payload, checkpoint}`
//! rather than as a new core field: the checkpoint is already "resume data,
//! opaque to everyone but the runner", and the IPC type stays stable for the
//! UI while it is built against mocks.

use riddle_core::{DigId, Job, JobKind, Priority};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct JobSpec {
    pub kind: JobKind,
    pub dig_id: Option<DigId>,
    pub label: String,
    pub payload: Value,
    /// Overrides `JobKind::default_priority`.
    pub priority: Option<Priority>,
    pub cancellable: bool,
}

impl JobSpec {
    pub fn new(kind: JobKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            dig_id: None,
            label: label.into(),
            payload: Value::Null,
            priority: None,
            cancellable: true,
        }
    }
    pub fn dig(mut self, dig: DigId) -> Self {
        self.dig_id = Some(dig);
        self
    }
    pub fn payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self
    }
    pub fn priority(mut self, p: Priority) -> Self {
        self.priority = Some(p);
        self
    }
    pub fn not_cancellable(mut self) -> Self {
        self.cancellable = false;
        self
    }
    pub fn effective_priority(&self) -> Priority {
        self.priority.unwrap_or(self.kind.default_priority())
    }
}

/// The contents of `Job.checkpoint`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobEnvelope {
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub checkpoint: Value,
}

impl JobEnvelope {
    pub fn of(job: &Job) -> Self {
        serde_json::from_value(job.checkpoint.clone()).unwrap_or_default()
    }
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Why everything is paused. Several can hold at once (asleep and offline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    Sleep,
    Offline,
    ProviderOutage,
    Budget,
    RateLimit,
    /// The persistence gate faulted (§11).
    Fault,
}

impl PauseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PauseReason::Sleep => "sleep",
            PauseReason::Offline => "offline",
            PauseReason::ProviderOutage => "provider_outage",
            PauseReason::Budget => "budget",
            PauseReason::RateLimit => "rate_limit",
            PauseReason::Fault => "fault",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [
            PauseReason::Sleep,
            PauseReason::Offline,
            PauseReason::ProviderOutage,
            PauseReason::Budget,
            PauseReason::RateLimit,
            PauseReason::Fault,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

/// The status reason for a job the user paused by hand; survives restarts.
pub const USER_PAUSE: &str = "user";

/// The limited resource a kind mostly consumes; at most one job per Dig runs
/// per class (§55), so two sync jobs never fight over one Dig's quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceClass {
    Provider,
    Judge,
    Llm,
    Local,
}

pub fn resource_class(kind: JobKind) -> ResourceClass {
    match kind {
        JobKind::InitialSync
        | JobKind::CatchUp
        | JobKind::IncrementalSync
        | JobKind::Projection => ResourceClass::Provider,
        JobKind::Sift
        | JobKind::HistoricalSift
        | JobKind::SignalBackfill
        | JobKind::ShadowEvaluation
        | JobKind::Replay
        | JobKind::TemporalSift => ResourceClass::Judge,
        JobKind::Expedition | JobKind::MeshSimulation => ResourceClass::Llm,
        JobKind::Embeddings | JobKind::BulkAction | JobKind::Maintenance => ResourceClass::Local,
    }
}

/// Kinds whose completion deserves a notification (§55).
pub fn is_long_running(kind: JobKind) -> bool {
    matches!(
        kind,
        JobKind::InitialSync | JobKind::HistoricalSift | JobKind::Replay | JobKind::Expedition
    )
}

/// A stable hash of a payload for dedupe, independent of key order.
pub fn payload_hash(v: &Value) -> String {
    fn canon(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let sorted: std::collections::BTreeMap<_, _> =
                    m.iter().map(|(k, v)| (k.clone(), canon(v))).collect();
                Value::Object(sorted.into_iter().collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
            other => other.clone(),
        }
    }
    riddle_core::sha256_hex(canon(v).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_payload_hash_ignores_key_order() {
        assert_eq!(
            payload_hash(&json!({"a": 1, "b": [1, {"c": 2, "d": 3}]})),
            payload_hash(&json!({"b": [1, {"d": 3, "c": 2}], "a": 1}))
        );
        assert_ne!(
            payload_hash(&json!({"a": 1})),
            payload_hash(&json!({"a": 2}))
        );
    }

    #[test]
    fn the_envelope_tolerates_a_bare_checkpoint() {
        let job_checkpoint = json!({"payload": {"x": 1}, "checkpoint": {"cursor": 5}});
        let e: JobEnvelope = serde_json::from_value(job_checkpoint).unwrap();
        assert_eq!(e.checkpoint["cursor"], 5);
        let missing: JobEnvelope = serde_json::from_value(json!({})).unwrap();
        assert_eq!(missing, JobEnvelope::default());
    }
}
