//! Finds: first-class entities derived from one or more messages (§41).

use crate::ids::{ClassificationId, DigId, FindId, MessageId, ThreadId, TraceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FindType {
    Respond,
    Decide,
    Review,
    Do,
    Pay,
    Attend,
    FollowUp,
    Know,
}

impl FindType {
    pub const ALL: [FindType; 8] = [
        FindType::Respond,
        FindType::Decide,
        FindType::Review,
        FindType::Do,
        FindType::Pay,
        FindType::Attend,
        FindType::FollowUp,
        FindType::Know,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            FindType::Respond => "respond",
            FindType::Decide => "decide",
            FindType::Review => "review",
            FindType::Do => "do",
            FindType::Pay => "pay",
            FindType::Attend => "attend",
            FindType::FollowUp => "follow_up",
            FindType::Know => "know",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|t| t.as_str().eq_ignore_ascii_case(s) || t.label().eq_ignore_ascii_case(s))
    }
    pub fn label(self) -> &'static str {
        match self {
            FindType::Respond => "Respond",
            FindType::Decide => "Decide",
            FindType::Review => "Review",
            FindType::Do => "Do",
            FindType::Pay => "Pay",
            FindType::Attend => "Attend",
            FindType::FollowUp => "Follow Up",
            FindType::Know => "Know",
        }
    }
    /// Dated Find types expire after their date.
    pub fn is_dated(self) -> bool {
        matches!(self, FindType::Attend | FindType::Pay)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FindStatus {
    Open,
    Snoozed,
    Resolved,
    Dismissed,
    Expired,
    Superseded,
}

impl FindStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            FindStatus::Open => "open",
            FindStatus::Snoozed => "snoozed",
            FindStatus::Resolved => "resolved",
            FindStatus::Dismissed => "dismissed",
            FindStatus::Expired => "expired",
            FindStatus::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ResolvedBy {
    User,
    ThreadState,
    Temporal,
    Director,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FindSource {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub dig_id: DigId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Find {
    pub id: FindId,
    pub find_type: FindType,
    pub title: String,
    pub summary: String,
    /// 0 lowest .. 100 highest; only Finds above the notify threshold interrupt.
    pub priority: u8,
    pub status: FindStatus,
    pub due_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<ResolvedBy>,
    pub sources: Vec<FindSource>,
    /// Dig ids the sources span; the same thread in two Digs is one Find.
    pub dig_ids: Vec<DigId>,
    /// Cross-Dig deduplication key (normally the RFC Message-ID root of the thread).
    pub identity_key: String,
    pub reason: String,
    pub confidence: Option<f64>,
    pub classification_id: Option<ClassificationId>,
    pub trace_id: Option<TraceId>,
    pub superseded_by: Option<FindId>,
}

/// What Sift proposes when a message deserves attention (§41). `riddle-finds`
/// joins it to an open Find with the same `identity_key` or creates one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FindProposal {
    pub find_type: FindType,
    pub title: String,
    pub summary: String,
    pub priority: u8,
    pub reason: String,
    /// Cross-Dig deduplication key: the RFC Message-ID root of the thread when
    /// present, else the provider thread id.
    pub identity_key: String,
    pub due_at: Option<DateTime<Utc>>,
    pub source: FindSource,
    pub confidence: Option<f64>,
    pub classification_id: Option<ClassificationId>,
    pub trace_id: Option<TraceId>,
}
