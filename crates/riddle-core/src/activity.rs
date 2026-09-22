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

/// Restore data keyed by affected entity id, so a bulk Activity can be undone
/// non-linearly (§50): the undo engine hands the Action's `undo` only the
/// entries for entities no later Activity touched. `shared` carries anything
/// that is not per-entity (a Mesh version, a label id). Actions that record
/// their restore data in this shape get partial undo for free; an opaque
/// restore value is all-or-nothing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreMap {
    pub entities: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub shared: serde_json::Value,
}

impl RestoreMap {
    pub fn new() -> Self { Self::default() }
    pub fn entity(mut self, id: impl std::fmt::Display, restore: serde_json::Value) -> Self {
        self.entities.insert(id.to_string(), restore);
        self
    }
    pub fn shared(mut self, shared: serde_json::Value) -> Self { self.shared = shared; self }
    pub fn to_value(&self) -> serde_json::Value { serde_json::to_value(self).unwrap_or(serde_json::Value::Null) }
    /// `Some` only when the value carries the per-entity shape.
    pub fn from_value(v: &serde_json::Value) -> Option<Self> {
        if v.get("entities")?.is_object() { serde_json::from_value(v.clone()).ok() } else { None }
    }
    /// Keep only the entities in `ids`.
    pub fn retain(&mut self, ids: &std::collections::HashSet<String>) { self.entities.retain(|k, _| ids.contains(k)); }
}

/// An entity whose provider state changed since the Activity; undo reports it
/// instead of guessing (§50).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UndoConflict {
    pub entity: EntityRef,
    pub detail: String,
}

/// What an undo did: restored, skipped ("handled since"), and conflicting
/// entities, plus the undo Activity when anything was restored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UndoReport {
    pub activity_id: ActivityId,
    pub restored: Vec<EntityRef>,
    pub skipped: Vec<SkippedEntity>,
    pub conflicts: Vec<UndoConflict>,
    /// `None` when nothing could be restored; the original stays undoable.
    pub undo_activity_id: Option<ActivityId>,
}

/// What an undo would do, shown in the in-place confirmation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UndoPreview {
    pub activity_id: ActivityId,
    pub label: String,
    pub summary: String,
    pub will_restore: Vec<EntityRef>,
    pub will_skip: Vec<SkippedEntity>,
    pub conflicts: Vec<UndoConflict>,
}
