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
    /// Redacted arguments: identifiers and counts.
    #[ts(type = "unknown")]
    pub args: serde_json::Value,
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
}
