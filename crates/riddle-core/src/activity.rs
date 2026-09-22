//! The Activity ledger (§47) and undo information (§50). Product Activity is
//! separate from software logs: it is what the user reads to answer "what
//! happened, who did it, why, and can I undo it".

use crate::classification::DecisionMechanism;
use crate::ids::{ActivityId, DigId, MeshId, TraceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Actor {
    User,
    Director,
    Sift,
    Mesh,
    Automation,
    System,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::User => "user",
            Actor::Director => "director",
            Actor::Sift => "sift",
            Actor::Mesh => "mesh",
            Actor::Automation => "automation",
            Actor::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EntityKind {
    Message,
    Thread,
    Find,
    Mesh,
    Signal,
    Stratum,
    Dig,
    Expedition,
    Draft,
    Job,
    Person,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
}

impl EntityRef {
    pub fn new(kind: EntityKind, id: impl std::fmt::Display) -> Self { Self { kind, id: id.to_string() } }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export)]
pub enum ActivityResult {
    Ok,
    Failed { error: String },
    /// Some affected entities succeeded, some did not; `skipped` says which and why.
    Partial { skipped: Vec<SkippedEntity> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkippedEntity {
    pub entity: EntityRef,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Provenance {
    pub mechanism: DecisionMechanism,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    pub trace_id: Option<TraceId>,
    /// e.g. "13 matched the Mesh on cached Signals. 1 needed a fresh evaluation."
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum UndoInfo {
    /// Restoration data the inverse Action needs, keyed by affected entity.
    Undoable {
        #[ts(type = "unknown")]
        restore: serde_json::Value,
        label: String,
    },
    NotUndoable { reason: String },
    Undone { at: DateTime<Utc>, by: Actor, undo_activity_id: ActivityId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Activity {
    pub id: ActivityId,
    pub actor: Actor,
    /// The Action name, e.g. `move_to_spoil`.
    pub action: String,
    /// Human sentence: "Sift moved 14 messages to Spoil".
    pub summary: String,
    pub why: Option<String>,
    /// Redacted inputs: identifiers and counts only.
    #[ts(type = "unknown")]
    pub inputs: serde_json::Value,
    pub affected: Vec<EntityRef>,
    pub result: ActivityResult,
    pub provenance: Option<Provenance>,
    pub undo: UndoInfo,
    pub dig_id: Option<DigId>,
    pub trace_id: Option<TraceId>,
    /// The Activity this one undid, when it is an undo.
    pub undoes: Option<ActivityId>,
    pub created_at: DateTime<Utc>,
}
