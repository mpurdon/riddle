//! Digs and Dig Groups: connected accounts and their policy scope.

use crate::ids::DigId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which provider backs a Dig. `Fixture` is the synthetic mailbox used by
/// tests, Demo Mode and the first build before a Gmail OAuth client exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderKind {
    Gmail,
    Fixture,
}

/// What may leave the device for this Dig (`ARCHITECTURE.md` §57).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PrivacyMode {
    /// Deterministic facts, Gmail categories and local embeddings only.
    LocalOnly,
    /// The judge sees normalized message state; everything else local.
    #[default]
    CloudJudgment,
    /// Judge and LLM, with redacted Context.
    CloudReasoning,
}

/// How Riddle's decisions are written back into the provider (§15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectionMode {
    #[default]
    Full,
    LabelsOnly,
    None,
}

/// Per-Dig runtime status shown in the Task Centre (§5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum DigStatus {
    Synced,
    CatchingUp {
        remaining: u64,
        total: u64,
    },
    Sifting {
        remaining: u64,
    },
    Projection {
        pending: u64,
    },
    Degraded {
        reason: String,
    },
    /// The Shadow Period is running; proposals are recorded, nothing is hidden.
    Shadow {
        day: u32,
        of: u32,
    },
    NotConnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Dig {
    pub id: DigId,
    pub name: String,
    pub email: String,
    pub provider: ProviderKind,
    /// The Dig Group name, if any. A Dig belongs to at most one group.
    pub group: Option<String>,
    pub privacy: PrivacyMode,
    pub projection: ProjectionMode,
    pub status: DigStatus,
    /// When the Shadow Period started; `None` until the first sync.
    pub shadow_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Switched off by the user: not synced, not sifted, left out of every
    /// list, count, notification, simulation and the Director, with its mail
    /// and decisions kept. Removing an account purges it; this was added so a
    /// fixture account could stop cluttering the real one without that.
    #[serde(default)]
    pub disabled: bool,
    /// Computed at read time from the account's settings and the build;
    /// never stored. `None` only for a Dig read straight from the store.
    #[serde(default)]
    #[ts(optional)]
    pub mailbox: Option<MailboxEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DigGroup {
    pub name: String,
    pub digs: Vec<DigId>,
    /// Rules that name this group (`Mesh "X"`, `Stratum "Y"`): while any
    /// exist the group cannot be renamed or deleted, and the client says so
    /// on the control instead of re-deriving the list.
    #[serde(default)]
    pub used_by: Vec<String>,
}

/// What Sift's decisions do to this account's mailbox right now, computed
/// by the same functions that gate projection, so the client phrases one
/// typed answer instead of re-deriving the rule in four places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailboxEffect {
    pub writes: MailboxWrites,
    /// Why nothing is written although the projection setting would write.
    pub held: Option<HeldReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MailboxWrites {
    Nothing,
    Labels,
    LabelsAndArchive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HeldReason {
    /// The account's rung is below Act.
    Autonomy,
    /// A development build that is not armed.
    DevUnarmed,
}
