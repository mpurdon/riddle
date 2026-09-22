//! Action metadata, autonomy and confirmation tiers (§8, §51). The Action
//! layer enforces these; a dialog only presents them.

use crate::ids::{ActivityId, ConversationId, DigId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Autonomy {
    Observe,
    Suggest,
    Act,
    Trusted,
}

impl Autonomy {
    pub fn as_str(self) -> &'static str {
        match self {
            Autonomy::Observe => "observe",
            Autonomy::Suggest => "suggest",
            Autonomy::Act => "act",
            Autonomy::Trusted => "trusted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

/// How the user confirms, per §51.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConfirmationPolicy {
    /// No confirmation; Activity and Undo cover it.
    None,
    /// Confirm in place with Undo All for bulk.
    InPlaceUndo,
    /// Two-step showing the real recipients.
    TwoStep,
    /// Type the exact count.
    TypeCount,
    /// Press and hold with coaching text.
    PressHold,
    /// Explicit approval with the reason shown on the disabled control.
    Approval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActionMeta {
    pub name: String,
    pub description: String,
    pub mutating: bool,
    pub reversible: bool,
    pub external_side_effect: bool,
    pub risk: Risk,
    pub confirmation: ConfirmationPolicy,
    /// The lowest autonomy at which a non-user principal may run it unprompted.
    pub minimum_autonomy: Autonomy,
    /// Egress Actions are approval-gated once a task has read untrusted content (§58).
    pub egress: bool,
    /// Read-only Actions are exposed to MCP first (§10).
    pub read_only: bool,
}

/// Who is asking. Prompts never replace authorization; the principal does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Principal {
    User,
    Director { conversation_id: ConversationId, read_untrusted: bool },
    Expedition { expedition_id: crate::ids::ExpeditionId, read_untrusted: bool },
    Sift,
    Temporal,
    Cli,
    Mcp,
    System,
}

impl Principal {
    pub fn actor(&self) -> crate::activity::Actor {
        use crate::activity::Actor;
        match self {
            Principal::User | Principal::Cli => Actor::User,
            Principal::Director { .. } | Principal::Expedition { .. } => Actor::Director,
            Principal::Sift => Actor::Sift,
            Principal::Temporal | Principal::System => Actor::System,
            Principal::Mcp => Actor::Automation,
        }
    }
    pub fn has_read_untrusted(&self) -> bool {
        matches!(self, Principal::Director { read_untrusted: true, .. } | Principal::Expedition { read_untrusted: true, .. })
    }
    pub fn is_user(&self) -> bool { matches!(self, Principal::User) }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActionInvocation {
    pub action: String,
    #[ts(type = "unknown")]
    pub args: serde_json::Value,
    pub principal: Principal,
    pub dig_id: Option<DigId>,
    /// Retries with the same key produce the same effect once (§52).
    pub idempotency_key: Option<String>,
    /// Set when the user has confirmed a pending action.
    pub confirmed: bool,
    /// The pending action id being confirmed, if any.
    pub pending_id: Option<String>,
}

/// Returned in the detail of a `needs_confirmation` error and stored until confirmed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PendingAction {
    pub id: String,
    pub action: String,
    #[ts(type = "unknown")]
    pub args: serde_json::Value,
    /// The Action layer's real arguments as the user must see them: actual
    /// recipients, actual rule text, actual count.
    #[ts(type = "unknown")]
    pub real_arguments: serde_json::Value,
    pub summary: String,
    pub confirmation: ConfirmationPolicy,
    pub reason: String,
    pub principal: Principal,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActionOutcome {
    #[ts(type = "unknown")]
    pub result: serde_json::Value,
    pub activity_id: Option<ActivityId>,
    /// True when the invocation was a replay of an idempotent key.
    pub replayed: bool,
}
