//! Director conversations. Conversations are not state (§43): outcomes
//! become Meshes, Signals, Expeditions, Finds and Activity. The transcript is
//! kept for context and for the Director Inspector.

use crate::ids::{ActivityId, ConversationId, DigId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TurnRole {
    User,
    Director,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolCall {
    pub id: String,
    pub action: String,
    /// Redacted arguments: identifiers and counts. What events, logs and
    /// Activity carry.
    #[ts(type = "unknown")]
    pub args: serde_json::Value,
    /// The arguments exactly as the Director sent them: a Mesh's text, a
    /// search's words. Kept in the conversation record, which is local and
    /// already holds the user's words in full; the redacted `args` had hidden
    /// the Payments text the Director simulated, so nobody could check its
    /// proposal against what it had tried (12 Oct 2026).
    #[serde(default)]
    #[ts(type = "unknown")]
    pub input: serde_json::Value,
    /// What the tool returned, compact: ids and counts for a read that
    /// carries mail, the result itself (cut at 4,000 characters) otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown", optional)]
    pub output: Option<serde_json::Value>,
    pub ok: bool,
    pub summary: String,
    pub activity_id: Option<ActivityId>,
    pub latency_ms: u64,
    /// True when the result carried mail content, which marks the task as
    /// having read untrusted input (§58).
    pub untrusted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Turn {
    pub role: TurnRole,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub created_at: DateTime<Utc>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
    /// Set on a Director turn that is an error, not an answer, so the client
    /// can count down to a retry the provider named and offer one otherwise.
    #[serde(default)]
    #[ts(optional)]
    pub failure: Option<TurnFailure>,
    /// Set on a Director turn a limit cut short ("the turn's limit of 40
    /// tool rounds is reached"), so the client can offer to continue.
    #[serde(default)]
    #[ts(optional)]
    pub stopped_early: Option<String>,
}

/// Why a Director turn failed. `retry_after_secs` comes from the provider
/// (a circuit breaker's wait, a rate limit's Retry-After) when there is one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TurnFailure {
    /// The error taxonomy's kind (ARCHITECTURE.md §8), typed so the client
    /// switches on it rather than on a string it has to keep in step.
    pub kind: riddle_errors::ErrorKind,
    pub retry_after_secs: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Conversation {
    pub id: ConversationId,
    pub title: String,
    pub turns: Vec<Turn>,
    /// Once true, egress Actions are approval-gated for this conversation.
    pub read_untrusted: bool,
    pub dig_ids: Vec<DigId>,
    pub model_id: Option<String>,
    pub prompt_version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// What the user allowed beyond a turn's usual limits by saying so
    /// ("continue for 20", "keep going"); the Director reads it per turn.
    #[serde(default)]
    #[ts(optional)]
    pub allowance: Option<ToolAllowance>,
}

/// Room the user gave the Director past its per-turn limits. The limits
/// exist to stop a runaway turn, not to end useful work, so the person can
/// lift them in words, and the Director parses those words without a model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolAllowance {
    /// No round limit until a new conversation; the cost limit still holds.
    pub keep_going: bool,
    /// The next turn only: this many tool calls.
    pub next_turn_tool_calls: Option<u32>,
    /// The next turn only: this cost limit in place of the usual one.
    pub next_turn_cost_usd: Option<f64>,
}

/// One Director conversation's whole story: its turns with every tool call
/// in full, the proposals it made and what became of each, and the Activity
/// linked to it (its proposals and previews, and what the user applied from
/// them). The Payments drafts of 12 Oct 2026 had to be pieced together from
/// redacted tool calls, a pending list that forgot declined proposals, and
/// an Activity ledger with no Director in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DirectorAudit {
    pub conversation: Conversation,
    pub proposals: Vec<crate::ProposalRecord>,
    pub activity: Vec<crate::Activity>,
}
