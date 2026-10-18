//! Digs and Dig Groups: connected accounts and their policy scope.

use crate::ids::DigId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which provider backs a Dig. `Fixture` is the synthetic mailbox used by
/// tests, Demo Mode and the first build before a Gmail OAuth client exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderKind {
    Gmail,
    Fixture,
}

/// What may leave the device for this Dig (`ARCHITECTURE.md` §57).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PrivacyMode {
    /// Deterministic facts, Gmail categories and local embeddings only.
    LocalOnly,
    /// The judge sees normalized message state; everything else local.
    #[default]
    CloudJudgment,
    /// Judge and LLM, with redacted Context.
    CloudReasoning,
}

/// How Riddle's decisions are written back into the provider (§15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectionMode {
    #[default]
    Full,
    LabelsOnly,
    None,
}

/// Why an account is not syncing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SyncFailure {
    /// The provider refuses the account's sign-in: sign in again.
    SignIn,
    /// Sign-in cannot work at all: the app has no OAuth client to sign in
    /// with. A wiped client id read as an expired sign-in, and "Sign in
    /// again" could only fail the same way.
    NotSetUp,
    /// A Workspace administrator blocks Riddle.
    AdminBlocked,
    /// The provider asked Riddle to slow down.
    RateLimited,
    /// The provider could not be reached or answered with an error.
    Unreachable,
    /// Anything else.
    Other,
}

impl SyncFailure {
    pub fn from_kind(kind: riddle_errors::ErrorKind) -> Self {
        use riddle_errors::ErrorKind as K;
        match kind {
            K::ProviderAuth => SyncFailure::SignIn,
            K::ProviderAdminBlocked => SyncFailure::AdminBlocked,
            K::ProviderRateLimited => SyncFailure::RateLimited,
            K::Provider => SyncFailure::Unreachable,
            _ => SyncFailure::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SyncFailure::SignIn => "sign_in",
            SyncFailure::NotSetUp => "not_set_up",
            SyncFailure::AdminBlocked => "admin_blocked",
            SyncFailure::RateLimited => "rate_limited",
            SyncFailure::Unreachable => "unreachable",
            SyncFailure::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "sign_in" => SyncFailure::SignIn,
            "not_set_up" => SyncFailure::NotSetUp,
            "admin_blocked" => SyncFailure::AdminBlocked,
            "rate_limited" => SyncFailure::RateLimited,
            "unreachable" => SyncFailure::Unreachable,
            _ => SyncFailure::Other,
        }
    }

    /// How long it must last before the account says so: a dropped
    /// connection or a rate limit usually clears on the next attempt; a
    /// refused sign-in never does.
    pub fn shows_after(self) -> chrono::Duration {
        match self {
            SyncFailure::RateLimited | SyncFailure::Unreachable => chrono::Duration::minutes(15),
            _ => chrono::Duration::zero(),
        }
    }

    /// One line for the banner, the notification and the Task Centre.
    pub fn sentence(self) -> &'static str {
        match self {
            SyncFailure::SignIn => "the sign-in has expired; sign in again to resume syncing",
            SyncFailure::NotSetUp => "Gmail sign-in is not set up: the OAuth client id is missing; add it under Settings → Accounts → Google sign-in",
            SyncFailure::AdminBlocked => {
                "a Workspace administrator blocks Riddle from this account"
            }
            SyncFailure::RateLimited => "Gmail is limiting requests; Riddle keeps retrying",
            SyncFailure::Unreachable => "Gmail cannot be reached; Riddle keeps retrying",
            SyncFailure::Other => "syncing fails; see Developer → Logs",
        }
    }
}

/// Per-Dig runtime status shown in the Task Centre (§5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum DigStatus {
    Synced,
    CatchingUp {
        remaining: u64,
        total: u64,
    },
    Sifting {
        remaining: u64,
    },
    Projection {
        pending: u64,
    },
    Degraded {
        reason: String,
    },
    /// Syncing has been failing: since when, why, and when the account last
    /// synced. A failed sync used to be logged and the job marked completed,
    /// so a real account showed "Caught up" through two weeks of expired
    /// sign-in. Sign-in and admin blocks show at once; a network or rate
    /// limit only after it has lasted (`SyncFailure::shows_after`).
    SyncFailing {
        failure: SyncFailure,
        since: chrono::DateTime<chrono::Utc>,
        last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// The Shadow Period is running; proposals are recorded, nothing is hidden.
    Shadow {
        day: u32,
        of: u32,
    },
    NotConnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Dig {
    pub id: DigId,
    pub name: String,
    pub email: String,
    pub provider: ProviderKind,
    /// The Dig Group name, if any. A Dig belongs to at most one group.
    pub group: Option<String>,
    pub privacy: PrivacyMode,
    pub projection: ProjectionMode,
    pub status: DigStatus,
    /// When the Shadow Period started; `None` until the first sync.
    pub shadow_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Switched off by the user: not synced, not sifted, left out of every
    /// list, count, notification, simulation and the Director, with its mail
    /// and decisions kept. Removing an account purges it; this was added so a
    /// fixture account could stop cluttering the real one without that.
    #[serde(default)]
    pub disabled: bool,
    /// How far back the history backfill fetches after the first sync, in
    /// days from now; `None` is the whole mailbox (§53). The first sync
    /// itself covers the sync window (90 days) whatever this says.
    #[serde(default)]
    #[ts(optional)]
    pub history_days: Option<u32>,
    /// Computed at read time from the account's settings and the build;
    /// never stored. `None` only for a Dig read straight from the store.
    #[serde(default)]
    #[ts(optional)]
    pub mailbox: Option<MailboxEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DigGroup {
    pub name: String,
    pub digs: Vec<DigId>,
    /// Rules that name this group (`Mesh "X"`, `Stratum "Y"`): while any
    /// exist the group cannot be renamed or deleted, and the client says so
    /// on the control instead of re-deriving the list.
    #[serde(default)]
    pub used_by: Vec<String>,
}

/// What Sift's decisions do to this account's mailbox right now, computed
/// by the same functions that gate projection, so the client phrases one
/// typed answer instead of re-deriving the rule in four places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailboxEffect {
    pub writes: MailboxWrites,
    /// Why nothing is written although the projection setting would write.
    pub held: Option<HeldReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MailboxWrites {
    Nothing,
    Labels,
    LabelsAndArchive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HeldReason {
    /// The account's rung is below Act.
    Autonomy,
    /// A development build that is not armed.
    DevUnarmed,
}
