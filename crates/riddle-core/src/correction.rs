//! Corrections are structured feedback (§46); undo retracts them (§50).

use crate::classification::Disposition;
use crate::find::FindType;
use crate::ids::{ActivityId, CorrectionId, DigId, MeshId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CorrectionKind {
    Move,
    Retype,
    Dismiss,
    Restore,
    NeverHide,
    ExternalMove,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Correction {
    pub id: CorrectionId,
    pub kind: CorrectionKind,
    pub dig_id: DigId,
    pub message_id: MessageId,
    pub from_disposition: Option<Disposition>,
    pub to_disposition: Option<Disposition>,
    pub from_find_type: Option<FindType>,
    pub to_find_type: Option<FindType>,
    /// The Mesh version that made the corrected decision, if any.
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    pub activity_id: Option<ActivityId>,
    pub created_at: DateTime<Utc>,
    /// Set when the generating action was undone; retracted corrections teach nothing.
    pub retracted_at: Option<DateTime<Utc>>,
}
