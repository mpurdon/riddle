//! Types that exist for the clients: paging, the morning summary, the "Why?"
//! explanation, and the event stream the webview subscribes to.

use crate::activity::Activity;
use crate::classification::{DecisionMechanism, Disposition};
use crate::dig::DigStatus;
use crate::find::Find;
use crate::ids::{DigId, FindId, JobId, MessageId, TraceId};
use crate::job::Job;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageRequest {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl PageRequest {
    pub fn limit(&self) -> u32 { self.limit.unwrap_or(50).clamp(1, 500) }
    pub fn offset(&self) -> u64 { self.cursor.as_deref().and_then(|c| c.parse().ok()).unwrap_or(0) }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[ts(bound = "T: TS")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

impl<T> Page<T> {
    pub fn from_offset(items: Vec<T>, offset: u64, limit: u32, total: Option<u64>) -> Self {
        let next = if items.len() as u32 == limit { Some((offset + limit as u64).to_string()) } else { None };
        Self { items, next_cursor: next, total }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShadowSummary {
    pub day: u32,
    pub of: u32,
    pub would_spoil: u64,
    pub would_find: u64,
    pub disagreements: u64,
}

/// The morning summary (README "The Core Experience"): honest about whether
/// processing is complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Summary {
    pub as_of: DateTime<Utc>,
    pub caught_up: bool,
    pub since: DateTime<Utc>,
    pub arrived: u64,
    pub dig_count: u32,
    pub finds_open: u64,
    pub unresolved: u64,
    pub filed: u64,
    pub spoil: u64,
    pub contamination: u64,
    pub remaining_to_sift: u64,
    pub total_to_sift: u64,
    pub director_note: Option<String>,
    pub shadow: Option<ShadowSummary>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub projection_pending: u64,
    pub degraded: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalLine {
    pub name: String,
    pub version: u32,
    pub score: f64,
    pub selected: String,
    pub cached: bool,
}

/// The answer to "Why did this go to Spoil?" (§37, §93).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Explanation {
    pub message_id: MessageId,
    pub disposition: Disposition,
    pub mechanism: DecisionMechanism,
    pub mesh_name: Option<String>,
    pub mesh_version: Option<u32>,
    pub matched: Option<String>,
    pub signals: Vec<SignalLine>,
    pub gate: Option<String>,
    pub escalated: bool,
    pub llm_rationale: Option<String>,
    pub trace_id: TraceId,
    pub shadow: bool,
    pub reason: String,
}

/// Events emitted to the webview. They carry signals, not volume: a client
/// that needs the data asks for it (§7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum RiddleEvent {
    JobChanged { job: Job },
    JobProgress { job_id: JobId, done: u64, total: Option<u64>, step: String },
    DigStatusChanged { dig_id: DigId, status: DigStatus },
    FindCreated { find: Find },
    FindChanged { find_id: FindId },
    ActivityAppended { activity: Activity },
    MessageClassified { message_id: MessageId, dig_id: DigId, disposition: Disposition, shadow: bool },
    MessagesChanged { dig_id: DigId, count: u64 },
    SummaryChanged,
    StoreFault { message: String },
    StoreFaultCleared,
    Notify { title: String, body: String, find_id: Option<FindId> },
    UpdateAvailable { version: String },
    /// Developer overlay: a Sift stage completed for a message (Live Pipeline, §82).
    PipelineStage { message_id: MessageId, stage: String, ms: u64 },
    /// Director streaming: text as it arrives.
    DirectorDelta { conversation_id: crate::ids::ConversationId, text: String },
    /// Director invoked an Action.
    DirectorToolCall { conversation_id: crate::ids::ConversationId, call: crate::conversation::ToolCall },
    /// Director finished a turn (or failed with `error`).
    DirectorDone { conversation_id: crate::ids::ConversationId, error: Option<String> },
    /// A Director proposal (Mesh, Signal, Never Hidden entry) awaits the user.
    ProposalCreated { pending_id: String, summary: String },
    ExpeditionChanged { expedition_id: crate::ids::ExpeditionId },
    SettingsChanged,
}

impl RiddleEvent {
    pub const NAME: &'static str = "riddle://event";
}
