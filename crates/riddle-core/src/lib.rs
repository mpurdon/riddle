//! The Riddle domain model.
//!
//! Every type that crosses the IPC boundary, the CLI, or the MCP server is
//! defined here and exported to TypeScript with ts-rs (`cargo test -p
//! riddle-core` regenerates `apps/desktop/src/bindings/`). The vocabulary is
//! `ARCHITECTURE.md` §2; nothing in this crate talks to a provider, a model or
//! a database, which is what lets every other crate depend on it without
//! cycles. Cross-domain traits (the semantic judge, the escalation LLM, the
//! clock) live in `traits` so Sift depends on an interface, not a vendor.

pub mod action;
pub mod activity;
pub mod api;
pub mod classification;
pub mod conversation;
pub mod correction;
pub mod dig;
pub mod expedition;
pub mod facts;
pub mod find;
pub mod ids;
pub mod job;
pub mod mail;
pub mod mesh;
pub mod people;
pub mod signal;
pub mod stratum;
pub mod traits;
pub mod usage;

pub use action::*;
pub use activity::*;
pub use api::*;
pub use classification::*;
pub use conversation::*;
pub use correction::*;
pub use dig::*;
pub use expedition::*;
pub use facts::*;
pub use find::*;
pub use ids::*;
pub use job::*;
pub use mail::*;
pub use mesh::*;
pub use people::*;
pub use signal::*;
pub use stratum::*;
pub use traits::*;
pub use usage::*;

pub use chrono::{DateTime, Utc};

/// A stable content hash, used for cache keys and prompt/schema identities.
pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes.as_ref()))
}

/// The build stamp recorded in every decision trace (`ARCHITECTURE.md` §6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BuildStamp {
    pub version: String,
    pub git_sha: String,
    pub dirty: bool,
    pub built_at: String,
    pub profile: String,
}

impl BuildStamp {
    /// Values injected by `build.rs` in the desktop crate; the CLI uses the same.
    pub fn from_env_vars(version: &str, git_sha: Option<&str>, dirty: Option<&str>, built_at: Option<&str>, profile: &str) -> Self {
        Self {
            version: version.to_string(),
            git_sha: git_sha.unwrap_or("unknown").to_string(),
            dirty: dirty == Some("true"),
            built_at: built_at.unwrap_or("unknown").to_string(),
            profile: profile.to_string(),
        }
    }
}
