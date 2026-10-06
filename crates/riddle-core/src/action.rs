//! Action metadata, autonomy and confirmation tiers (§8, §51). The Action
//! layer enforces these; a dialog only presents them.

use crate::activity::EntityRef;
use crate::ids::{ActivityId, ConversationId, DigId};
use chrono::{DateTime, Utc};
use riddle_errors::{Result, RiddleError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An optional timestamp argument that must be RFC 3339 and after `now`: a
/// snooze's `until`, a follow-up's `dueAt`. A date already past would snooze
/// nothing or open a Find already overdue, so it is refused rather than
/// applied. `Ok(None)` when the key is absent or null.
pub fn future_timestamp(
    args: &serde_json::Value,
    key: &str,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    let Some(v) = args.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let at: DateTime<Utc> = v
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| RiddleError::invalid(format!("{key} must be an RFC 3339 timestamp")))?;
    if at <= now {
        return Err(RiddleError::invalid(format!("{key} must be in the future")));
    }
    Ok(Some(at))
}

/// `future_timestamp` for an argument the Action cannot do without.
pub fn required_future_timestamp(
    args: &serde_json::Value,
    key: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    future_timestamp(args, key, now)?
        .ok_or_else(|| RiddleError::invalid(format!("missing argument '{key}'")))
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS, Default,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Autonomy {
    #[default]
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
    /// Calls the provider while it runs (send, permanent delete,
    /// unsubscribe): the dev build's Hold / Ask / Allow gates the Action.
    pub external_side_effect: bool,
    /// Changes Riddle, and queues the matching mailbox change in the
    /// outbox (corrections, archive, Strata). The Action always runs; the
    /// queued write waits for the dev build's Hold / Ask / Allow. A
    /// correction was refused outright when it shared the flag above, though
    /// it never touched the mailbox itself.
    #[serde(default)]
    pub projected: bool,
    pub risk: Risk,
    pub confirmation: ConfirmationPolicy,
    /// The lowest autonomy at which a non-user principal may run it unprompted.
    pub minimum_autonomy: Autonomy,
    /// Egress Actions are approval-gated once a task has read untrusted content (§58).
    pub egress: bool,
    /// Read-only Actions are exposed to MCP first (§10).
    pub read_only: bool,
    /// Only a person (the app or the CLI) may run it: agents never see it
    /// and are refused before a pending confirmation could store its
    /// arguments. For Actions whose arguments are secrets.
    #[serde(default)]
    pub user_only: bool,
}

/// What a dev build may write to the connected mailbox (§59). Hold: nothing
/// leaves; corrections still count in Riddle and their mailbox writes wait
/// in the outbox. Ask: queued writes wait for one "send these N" from the
/// user, and an Action that calls the provider directly asks each time.
/// Allow: writes go as they would in a release build. A dev build starts in
/// Hold. It replaced a single armed switch, under which a batch of seven
/// corrections was refused seven times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderWrites {
    #[default]
    Hold,
    Ask,
    Allow,
}

impl ProviderWrites {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderWrites::Hold => "hold",
            ProviderWrites::Ask => "ask",
            ProviderWrites::Allow => "allow",
        }
    }
}

/// The dev build's mailbox writes and what waits on them, for Developer
/// and the "N changes waiting" prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailboxWritesState {
    pub dev: bool,
    pub writes: ProviderWrites,
    /// Queued mailbox writes on accounts that write to their mailbox.
    pub held: u64,
}

/// Who is asking. Prompts never replace authorization; the principal does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Principal {
    User,
    Director {
        conversation_id: ConversationId,
        read_untrusted: bool,
    },
    Expedition {
        expedition_id: crate::ids::ExpeditionId,
        read_untrusted: bool,
    },
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
        matches!(
            self,
            Principal::Director {
                read_untrusted: true,
                ..
            } | Principal::Expedition {
                read_untrusted: true,
                ..
            }
        )
    }
    pub fn is_user(&self) -> bool {
        matches!(self, Principal::User)
    }
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
    /// The Activity sentence, for the toast: "Moved 14 messages to Spoil".
    #[serde(default)]
    pub summary: String,
    /// What the Action touched, so the client refreshes by entity kind
    /// rather than by a table of action names.
    #[serde(default)]
    pub affected: Vec<EntityRef>,
}
