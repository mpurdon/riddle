//! Identifier newtypes. Provider ids (Gmail message and thread ids) are kept
//! as strings verbatim; Riddle-owned ids are UUIDv7 so they sort by creation.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS,
        )]
        #[serde(transparent)]
        #[ts(export, type = "string")]
        pub struct $name(pub String);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7().to_string())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }
        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

id_type!(DigId, "A connected account.");
id_type!(
    MessageId,
    "Riddle's id for a message; stable across resync (dig id + provider id hash)."
);
id_type!(ThreadId, "Riddle's id for a thread.");
id_type!(FindId, "A Find.");
id_type!(MeshId, "A Mesh, stable across versions.");
id_type!(SignalId, "A Signal, stable across versions.");
id_type!(StratumId, "A Stratum.");
id_type!(ActivityId, "An Activity ledger entry.");
id_type!(JobId, "A durable job.");
id_type!(TraceId, "A decision trace.");
id_type!(ExpeditionId, "An Expedition.");
id_type!(ClassificationId, "A classification record.");
id_type!(PersonId, "A person across Digs.");
id_type!(CorrectionId, "A structured correction.");
id_type!(ConversationId, "A Director conversation.");

impl MessageId {
    /// Deterministic id so a message re-fetched after a full resync keeps its
    /// classifications, Finds and Activity.
    pub fn for_provider(dig: &DigId, provider_message_id: &str) -> Self {
        Self(format!("{}:{}", dig.0, provider_message_id))
    }
    pub fn provider_id(&self) -> &str {
        self.0.rsplit_once(':').map(|(_, p)| p).unwrap_or(&self.0)
    }
}

impl ThreadId {
    pub fn for_provider(dig: &DigId, provider_thread_id: &str) -> Self {
        Self(format!("{}:{}", dig.0, provider_thread_id))
    }
    pub fn provider_id(&self) -> &str {
        self.0.rsplit_once(':').map(|(_, p)| p).unwrap_or(&self.0)
    }
}
