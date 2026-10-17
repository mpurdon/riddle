//! A suggestion Riddle makes about its own rules, for the Director to voice
//! and the user to take or leave. Nothing here changes a Mesh: a suggestion
//! becomes a proposal with its rule text and a simulation, and the user
//! says yes (§28, §46).

use crate::{FindId, MeshId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A sender whose open Finds repeat one subject, numbers aside, under a
/// route without `+ Replaces`. A smart lock's "batteries are critically
/// low" every few days stood as 33 Do Finds, and a registrar's reminders
/// as 30, before anyone noticed (16 Oct 2026); the fix each time was
/// `+ Replaces: same subject` on the route, which the Director can now
/// propose for any sender from this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SeriesSuggestion {
    pub mesh_id: MeshId,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    /// The route as written, when the Mesh still has it.
    pub route: Option<String>,
    pub sender: String,
    /// Open Finds in the runs.
    pub count: u32,
    /// Runs side by side, one per subject with its numbers set aside.
    pub runs: u32,
    /// One subject from the largest run.
    pub sample: String,
    /// The sentence the Director says.
    pub text: String,
    pub find_ids: Vec<FindId>,
}
