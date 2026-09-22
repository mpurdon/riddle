//! Expeditions: durable delegated agent work (§42).

use crate::activity::EntityRef;
use crate::ids::{DigId, ExpeditionId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExpeditionScope {
    pub dig_ids: Vec<DigId>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// Action names the Expedition may invoke.
    pub tools_allowed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExpeditionBudget {
    pub cost_usd: f64,
    pub seconds: u64,
    pub tool_calls: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExpeditionStatus {
    Planned,
    Running,
    Paused,
    Completed,
    Partial,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanStep {
    pub n: u32,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Expedition {
    pub id: ExpeditionId,
    pub goal: String,
    pub scope: ExpeditionScope,
    pub budget: ExpeditionBudget,
    pub spent: ExpeditionBudget,
    pub status: ExpeditionStatus,
    pub plan: Vec<PlanStep>,
    #[ts(type = "unknown")]
    pub checkpoint: serde_json::Value,
    pub evidence: Vec<EntityRef>,
    pub findings: Vec<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}
