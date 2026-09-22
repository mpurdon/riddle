//! Dispositions, classifications and decision traces: what Riddle decided
//! about a message and everything that participated (`ARCHITECTURE.md` §2,
//! §49, §60). A classification without a trace is a violation of §92.

use crate::find::FindType;
use crate::ids::{ClassificationId, DigId, MeshId, MessageId, TraceId};
use crate::signal::SignalEvaluation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Every message has exactly one disposition, and Unresolved is one of them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Disposition {
    Find { find_type: FindType },
    Filed,
    Spoil,
    Contamination,
    Unresolved,
}

impl Disposition {
    pub fn kind(&self) -> DispositionKind {
        match self {
            Disposition::Find { .. } => DispositionKind::Find,
            Disposition::Filed => DispositionKind::Filed,
            Disposition::Spoil => DispositionKind::Spoil,
            Disposition::Contamination => DispositionKind::Contamination,
            Disposition::Unresolved => DispositionKind::Unresolved,
        }
    }
    /// Whether this disposition removes the message from the user's view.
    pub fn hides(&self) -> bool {
        matches!(
            self,
            Disposition::Spoil | Disposition::Contamination | Disposition::Filed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DispositionKind {
    Find,
    Filed,
    Spoil,
    Contamination,
    Unresolved,
}

impl DispositionKind {
    pub const ALL: [DispositionKind; 5] = [
        DispositionKind::Find,
        DispositionKind::Filed,
        DispositionKind::Spoil,
        DispositionKind::Contamination,
        DispositionKind::Unresolved,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            DispositionKind::Find => "find",
            DispositionKind::Filed => "filed",
            DispositionKind::Spoil => "spoil",
            DispositionKind::Contamination => "contamination",
            DispositionKind::Unresolved => "unresolved",
        }
    }
}

/// Which system made a decision (§49).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionMechanism {
    Deterministic,
    Gate,
    Jev,
    Mesh,
    Llm,
    Temporal,
    Director,
    User,
    Provider,
}

/// The current classification of one message. Re-classification writes a new
/// row and supersedes the old one; history is kept for replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Classification {
    pub id: ClassificationId,
    pub message_id: MessageId,
    pub dig_id: DigId,
    pub disposition: Disposition,
    pub strata: Vec<String>,
    pub notify: bool,
    pub mechanism: DecisionMechanism,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    /// The route or gate that matched, as text, for the "Why?" answer.
    pub matched_rule: Option<String>,
    pub confidence: Option<f64>,
    pub reason: String,
    pub trace_id: TraceId,
    /// True during the Shadow Period: recorded, shown, not applied (§51).
    pub shadow: bool,
    pub created_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
}

/// Per-stage timing for one message, in milliseconds (§68).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TraceTiming {
    pub sync_ms: u64,
    pub facts_ms: u64,
    pub judge_ms: u64,
    pub mesh_ms: u64,
    pub llm_ms: u64,
    pub persist_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TraceCost {
    pub judge_input_tokens: u64,
    pub judge_questions: u32,
    /// USD; `None` means unpriced, never free (§56).
    pub judge_cost_usd: Option<f64>,
    pub llm_input_tokens: u64,
    pub llm_output_tokens: u64,
    pub llm_cost_usd: Option<f64>,
}

/// What the LLM escalation returned (§39). It refines Signals and gives a
/// rationale; it never returns a disposition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EscalationRecord {
    pub reason: String,
    pub model_id: String,
    pub refined: Vec<SignalEvaluation>,
    pub rationale: String,
    pub latency_ms: u64,
}

/// The structured reasoning artifact behind one classification (§60).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecisionTrace {
    pub id: TraceId,
    pub message_id: MessageId,
    pub dig_id: DigId,
    pub objective: String,
    pub strategy: String,
    pub facts: crate::facts::Facts,
    pub gate: Option<String>,
    pub signals: Vec<SignalEvaluation>,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    pub matched_rule: Option<String>,
    pub guard_vetoes: Vec<String>,
    pub escalation: Option<EscalationRecord>,
    pub conclusion: Disposition,
    pub confidence: Option<f64>,
    pub mechanism: DecisionMechanism,
    pub timing: TraceTiming,
    pub cost: TraceCost,
    pub judge_identity: Option<String>,
    pub app_version: String,
    pub build_stamp: String,
    pub shadow: bool,
    pub created_at: DateTime<Utc>,
}
