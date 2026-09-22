//! Provider-shaped mail entities. Gmail concepts are preserved rather than
//! flattened to IMAP (`ARCHITECTURE.md` §16); Riddle's own understanding lives
//! in `classification`, `find` and friends, never here (§14).

use crate::ids::{DigId, MessageId, ThreadId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Address {
    pub email: String,
    pub name: Option<String>,
}

impl Address {
    pub fn new(email: impl Into<String>, name: Option<&str>) -> Self {
        Self {
            email: email.into().to_lowercase(),
            name: name.map(|s| s.to_string()),
        }
    }
    pub fn domain(&self) -> &str {
        self.email.rsplit_once('@').map(|(_, d)| d).unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    /// Provider attachment id, for lazy fetch.
    pub provider_id: Option<String>,
    pub content_id: Option<String>,
}

/// Provider-level authentication results parsed from headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AuthVerdict {
    Pass,
    Fail,
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthenticationResults {
    pub spf: AuthVerdict,
    pub dkim: AuthVerdict,
    pub dmarc: AuthVerdict,
}

/// The headers Sift reads. Kept as a struct, not a map, so the deterministic
/// facts vocabulary is checkable at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MessageHeaders {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub list_id: Option<String>,
    pub list_unsubscribe: Option<String>,
    pub precedence: Option<String>,
    pub auto_submitted: Option<String>,
    pub reply_to: Option<Address>,
    pub authentication: AuthenticationResults,
}

/// A message as synced from the provider, with bodies fetched lazily.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Message {
    pub id: MessageId,
    pub dig_id: DigId,
    pub thread_id: ThreadId,
    pub provider_id: String,
    pub subject: String,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub date: DateTime<Utc>,
    pub snippet: String,
    /// Provider labels verbatim (INBOX, SENT, CATEGORY_PROMOTIONS, user labels).
    pub labels: Vec<String>,
    pub headers: MessageHeaders,
    pub attachments: Vec<AttachmentMeta>,
    pub size_bytes: u64,
    /// True when the user sent it (SENT label or from an own address).
    pub is_sent: bool,
    pub has_body: bool,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub synced_at: DateTime<Utc>,
}

/// The paged list row: everything a list needs and nothing a body carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MessageSummary {
    pub id: MessageId,
    pub dig_id: DigId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub from: Address,
    pub date: DateTime<Utc>,
    pub snippet: String,
    pub labels: Vec<String>,
    pub is_sent: bool,
    pub has_attachments: bool,
    pub disposition: Option<crate::classification::Disposition>,
    pub strata: Vec<String>,
    pub find_id: Option<crate::ids::FindId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Thread {
    pub id: ThreadId,
    pub dig_id: DigId,
    pub provider_id: String,
    pub subject: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Draft {
    pub id: String,
    pub dig_id: DigId,
    pub provider_id: Option<String>,
    pub thread_id: Option<ThreadId>,
    pub in_reply_to: Option<MessageId>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body_text: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderLabel {
    pub id: String,
    pub name: String,
    /// True for labels Riddle created (carry the managed-by marker).
    pub managed_by_riddle: bool,
    pub system: bool,
}
