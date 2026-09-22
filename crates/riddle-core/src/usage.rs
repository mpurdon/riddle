//! The usage ledger (§56): every model call is a row with task-local
//! attribution. Anything that cannot be priced is reported as unpriced.

use crate::ids::{DigId, TraceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UsageTask {
    Sift,
    Director,
    Expedition,
    Simulation,
    Backfill,
    SignalTest,
    Probe,
}

impl UsageTask {
    pub fn as_str(self) -> &'static str {
        match self {
            UsageTask::Sift => "sift",
            UsageTask::Director => "director",
            UsageTask::Expedition => "expedition",
            UsageTask::Simulation => "simulation",
            UsageTask::Backfill => "backfill",
            UsageTask::SignalTest => "signal_test",
            UsageTask::Probe => "probe",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsageRow {
    pub id: String,
    pub at: DateTime<Utc>,
    /// `jev`, `bedrock`, `null`.
    pub provider: String,
    pub model: String,
    pub task: UsageTask,
    pub dig_id: Option<DigId>,
    pub trace_id: Option<TraceId>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
    pub latency_ms: u64,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsageSummary {
    pub since: DateTime<Utc>,
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub priced_usd: f64,
    pub unpriced_calls: u64,
    pub by_task: Vec<(String, f64, u64)>,
    pub by_provider: Vec<(String, f64, u64)>,
}
