//! The provider contract. Gmail concepts are preserved (labels, threads,
//! drafts, history) rather than flattened to an IMAP-like minimum (§16), and
//! every method maps its failures to the §8 taxonomy so the sync engine and
//! the Action layer can park, back off, or ask for sign-in without sniffing
//! text.
//!
//! Labels are provider ids everywhere in this trait (`INBOX`, `SENT`,
//! `Label_42`). Names are resolved to ids through `ensure_label`, which is
//! what projection does for `Riddle/Spoil` and friends. The fixture uses the
//! name as the id so tests can read them.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use riddle_core::{Address, Message, ProviderKind, ProviderLabel};
use riddle_errors::{Result, RiddleError};
use serde::{Deserialize, Serialize};

/// The prefix that marks a label as managed by Riddle (§15). Gmail labels
/// carry no metadata, so the name is the marker.
pub const MANAGED_PREFIX: &str = "Riddle/";

pub fn is_managed_label_name(name: &str) -> bool {
    name.starts_with(MANAGED_PREFIX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub email: String,
    pub aliases: Vec<String>,
    pub name: Option<String>,
    /// The provider's current change cursor, if it reports one at profile
    /// time (Gmail's `historyId`). Initial sync records it before listing so
    /// nothing that arrives during the listing is missed.
    pub history_cursor: Option<String>,
    pub messages_total: Option<u64>,
}

/// How much of a message to fetch. Metadata is cheap and enough for Sift's
/// deterministic facts; bodies come later, newest first (§53).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Detail {
    Metadata,
    Full,
}

/// One page of the newest-first listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPage {
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub page_token: Option<String>,
    /// Ids per page; providers clamp to their own maximum.
    pub max: u32,
    /// Restrict to a label id (e.g. INBOX); `None` lists everything but spam and trash.
    pub label: Option<String>,
}

impl ListPage {
    pub fn newest(max: u32) -> Self {
        Self {
            max,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRef {
    pub provider_id: String,
    pub thread_provider_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdPage {
    pub refs: Vec<MessageRef>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u64>,
}

/// One provider-side change since a cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Added {
        id: String,
    },
    Deleted {
        id: String,
    },
    LabelsChanged {
        id: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
}

impl Change {
    pub fn id(&self) -> &str {
        match self {
            Change::Added { id } | Change::Deleted { id } | Change::LabelsChanged { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPage {
    pub changes: Vec<Change>,
    /// The cursor to store once `changes` are applied.
    pub next_cursor: Option<String>,
    /// The cursor is too old; a full resync is required (§53).
    pub expired: bool,
}

/// What Riddle sends. The Action layer builds it from a `Draft` and the
/// store, so the provider never needs to look anything up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
    pub thread_provider_id: Option<String>,
    /// RFC 5322 Message-ID of the message being replied to.
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

impl OutgoingMessage {
    pub fn recipients(&self) -> Vec<&Address> {
        self.to
            .iter()
            .chain(self.cc.iter())
            .chain(self.bcc.iter())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentMessage {
    pub provider_id: String,
    pub thread_provider_id: String,
}

/// The mail provider behind a Dig.
#[async_trait]
pub trait MailProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    async fn profile(&self) -> Result<ProviderProfile>;
    async fn list_labels(&self) -> Result<Vec<ProviderLabel>>;
    /// Find or create a label by name. Managed names carry the marker.
    async fn ensure_label(&self, name: &str) -> Result<ProviderLabel>;
    /// Newest first.
    async fn list_message_ids(&self, page: &ListPage) -> Result<IdPage>;
    async fn get_message(&self, provider_id: &str, detail: Detail) -> Result<Message>;
    /// Missing ids are skipped, not errors: a message deleted between the
    /// listing and the fetch is normal.
    async fn get_messages_batch(&self, ids: &[String], detail: Detail) -> Result<Vec<Message>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.get_message(id, detail).await {
                Ok(m) => out.push(m),
                Err(e) if e.kind == riddle_errors::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }
    async fn history(&self, cursor: &str) -> Result<HistoryPage>;
    /// Batch label change; ids and label ids. Providers split large batches.
    async fn modify_labels(&self, ids: &[String], add: &[String], remove: &[String]) -> Result<()>;
    /// Returns the provider draft id.
    async fn create_draft(&self, message: &OutgoingMessage) -> Result<String>;
    async fn update_draft(
        &self,
        provider_draft_id: &str,
        message: &OutgoingMessage,
    ) -> Result<String>;
    async fn delete_draft(&self, provider_draft_id: &str) -> Result<()>;
    async fn send_draft(&self, provider_draft_id: &str) -> Result<SentMessage>;
    async fn send(&self, message: &OutgoingMessage) -> Result<SentMessage>;
    async fn trash(&self, ids: &[String]) -> Result<()>;
    /// Provider-side search, when the provider has one.
    async fn search(&self, _query: &str, _max: u32) -> Result<Vec<MessageRef>> {
        Err(RiddleError::invalid(
            "this provider has no server-side search",
        ))
    }
}
