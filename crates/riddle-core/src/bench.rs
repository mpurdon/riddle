//! Model comparison: which LLM suits which job, measured on the person's
//! own mail. Two jobs use a model. Escalation re-estimates Signals for a
//! message the judge could not settle, and the person's own decisions are
//! the right answer to score it against. The Director answers questions
//! and proposes rules, where there is no right answer, so its answers are
//! shown side by side and the person picks the better one.
//!
//! A roster of candidate models is probed for access on a schedule: new
//! models are often listed by the provider long before an account may call
//! them (Claude 5.5 and GPT-6 Luna on Bedrock in October 2026), and a model
//! joins comparisons the day it becomes usable. Outputs are recorded, never
//! applied.

use crate::classification::Disposition;
use crate::ids::{DigId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A job a model does in Riddle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BenchTask {
    Escalation,
    Director,
}

impl BenchTask {
    pub fn as_str(self) -> &'static str {
        match self {
            BenchTask::Escalation => "escalation",
            BenchTask::Director => "director",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "escalation" => Some(BenchTask::Escalation),
            "director" => Some(BenchTask::Director),
            _ => None,
        }
    }
}

/// Where an output came from: a benchmark the person ran, or a challenger
/// run alongside a real escalation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BenchSource {
    Benchmark,
    Shadow,
}

/// Whether the account may call a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ModelAccess {
    /// Not checked yet.
    Unchecked,
    /// A test call answered.
    Usable,
    /// The provider lists it but this account may not call it yet.
    NotEnabled,
    /// The test call failed some other way.
    Failing,
}

/// A candidate model and what the last access check found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BenchModel {
    /// The provider's id: `us.anthropic.claude-haiku-4-5-20251001-v1:0`.
    pub model_id: String,
    pub label: String,
    /// `Anthropic`, `OpenAI`.
    pub vendor: String,
    pub access: ModelAccess,
    pub access_detail: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
    /// The test call's round trip.
    #[ts(type = "number | null")]
    pub probe_latency_ms: Option<u64>,
    /// USD per million tokens, when the catalog has a verified price.
    pub price_in_per_mtok: Option<f64>,
    pub price_out_per_mtok: Option<f64>,
    /// The jobs this model does now.
    pub used_for: Vec<BenchTask>,
}

/// One model's answer to one comparison item. Never applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BenchOutput {
    pub id: String,
    pub task: BenchTask,
    pub source: BenchSource,
    pub model_id: String,
    pub dig_id: Option<DigId>,
    pub message_id: Option<MessageId>,
    /// For the Director: the question replayed, and its key.
    pub question: Option<String>,
    pub question_key: Option<String>,
    /// For escalation: what the Mesh routes decided on this model's Signals.
    pub disposition: Option<Disposition>,
    /// The rationale (escalation) or the answer (Director).
    pub text: String,
    #[ts(type = "number")]
    pub latency_ms: u64,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// How one model did at one job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BenchScore {
    pub model_id: String,
    pub task: BenchTask,
    #[ts(type = "number")]
    pub runs: u64,
    #[ts(type = "number")]
    pub errors: u64,
    /// Escalation: outputs on messages the person has decided themselves.
    #[ts(type = "number")]
    pub judged: u64,
    #[ts(type = "number")]
    pub correct: u64,
    /// Escalation: outputs that match what Riddle decided in production.
    #[ts(type = "number")]
    pub agrees_with_current: u64,
    /// Director: questions where the person picked this model's answer.
    #[ts(type = "number")]
    pub preferred: u64,
    #[ts(type = "number")]
    pub avg_latency_ms: u64,
    /// Average cost of 1,000 runs, when every run was priced.
    pub cost_per_1k: Option<f64>,
}

/// One escalation item, side by side: the person's answer, Riddle's, and
/// every model's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BenchRow {
    pub dig_id: DigId,
    pub message_id: MessageId,
    pub subject: String,
    pub from: String,
    /// The person's own decision, when there is one.
    pub truth: Option<Disposition>,
    /// What Riddle decided in production.
    pub current: Option<Disposition>,
    pub outputs: Vec<BenchOutput>,
}

/// One replayed Director question, side by side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DirectorComparison {
    pub question_key: String,
    pub question: String,
    pub outputs: Vec<BenchOutput>,
    /// The model whose answer the person picked.
    pub preferred: Option<String>,
}

/// The Models view in one read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BenchOverview {
    pub models: Vec<BenchModel>,
    pub scores: Vec<BenchScore>,
    /// Challengers run on every real escalation, under the daily cap.
    pub shadow: bool,
    pub spent_today_usd: f64,
    pub daily_cap_usd: f64,
    /// A benchmark or replay still running.
    pub running: bool,
}
