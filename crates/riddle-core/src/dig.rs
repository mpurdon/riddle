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
    CatchingUp { remaining: u64, total: u64 },
    Sifting { remaining: u64 },
    Projection { pending: u64 },
    Degraded { reason: String },
    /// The Shadow Period is running; proposals are recorded, nothing is hidden.
    Shadow { day: u32, of: u32 },
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DigGroup {
    pub name: String,
    pub digs: Vec<DigId>,
}
