//! Durable jobs, priority classes and the Task Centre (§54–§55).

use crate::dig::DigStatus;
use crate::ids::{DigId, JobId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Highest first. Lower classes yield under back-pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Priority {
    Interactive,
    Incoming,
    CatchUp,
    Projection,
    Backfill,
    Experiment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum JobKind {
    InitialSync,
    CatchUp,
    IncrementalSync,
    Sift,
    HistoricalSift,
    SignalBackfill,
    MeshSimulation,
    Projection,
    Expedition,
    Embeddings,
    BulkAction,
    Replay,
    ShadowEvaluation,
    TemporalSift,
    Maintenance,
}

impl JobKind {
    pub fn default_priority(self) -> Priority {
        match self {
            JobKind::BulkAction | JobKind::MeshSimulation => Priority::Interactive,
            JobKind::Sift | JobKind::IncrementalSync | JobKind::TemporalSift => Priority::Incoming,
            JobKind::CatchUp | JobKind::InitialSync => Priority::CatchUp,
            JobKind::Projection => Priority::Projection,
            JobKind::HistoricalSift
            | JobKind::SignalBackfill
            | JobKind::Embeddings
            | JobKind::Maintenance
            | JobKind::Expedition => Priority::Backfill,
            JobKind::Replay | JobKind::ShadowEvaluation => Priority::Experiment,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::InitialSync => "initial_sync",
            JobKind::CatchUp => "catch_up",
            JobKind::IncrementalSync => "incremental_sync",
            JobKind::Sift => "sift",
            JobKind::HistoricalSift => "historical_sift",
            JobKind::SignalBackfill => "signal_backfill",
            JobKind::MeshSimulation => "mesh_simulation",
            JobKind::Projection => "projection",
            JobKind::Expedition => "expedition",
            JobKind::Embeddings => "embeddings",
            JobKind::BulkAction => "bulk_action",
            JobKind::Replay => "replay",
            JobKind::ShadowEvaluation => "shadow_evaluation",
            JobKind::TemporalSift => "temporal_sift",
            JobKind::Maintenance => "maintenance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum JobStatus {
    Queued,
    Running,
    /// Paused with the reason shown in the Task Centre: sleep, offline, budget, rate limit, fault.
    Paused {
        reason: String,
    },
    Completed,
    Failed {
        error: String,
    },
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed { .. } | JobStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Progress {
    pub done: u64,
    /// `None` when the total is unknown; the UI shows an indeterminate bar.
    pub total: Option<u64>,
    pub unit: String,
    /// The named step in progress, e.g. "Fetching headers".
    pub step: String,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub priority: Priority,
    pub status: JobStatus,
    pub dig_id: Option<DigId>,
    pub label: String,
    pub progress: Progress,
    /// Resume data; opaque to everyone but the job's runner.
    #[ts(type = "unknown")]
    pub checkpoint: serde_json::Value,
    pub cancellable: bool,
    pub attempts: u32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OverallStatus {
    CaughtUp,
    CatchingUp,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DigStatusRow {
    pub dig_id: DigId,
    pub name: String,
    pub status: DigStatus,
}

/// The Task Centre snapshot (§55), a product surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskCentre {
    pub overall: OverallStatus,
    pub digs: Vec<DigStatusRow>,
    pub jobs: Vec<Job>,
    /// The persistence gate fault flag (§11), when set.
    pub store_fault: Option<String>,
    pub as_of: DateTime<Utc>,
}
