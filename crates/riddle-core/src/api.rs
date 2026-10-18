//! Types that exist for the clients: paging, the morning summary, the "Why?"
//! explanation, and the event stream the webview subscribes to.

use crate::action::Autonomy;
use crate::activity::Activity;
use crate::classification::{Classification, DecisionMechanism, DecisionTrace, Disposition};
use crate::dig::{DigStatus, ProjectionMode};
use crate::find::Find;
use crate::ids::{DigId, FindId, JobId, MeshId, MessageId, TraceId};
use crate::job::Job;
use crate::mesh::MeshStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageRequest {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl PageRequest {
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(50).clamp(1, 500)
    }
    pub fn offset(&self) -> u64 {
        self.cursor
            .as_deref()
            .and_then(|c| c.parse().ok())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[ts(bound = "T: TS")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

impl<T> Page<T> {
    pub fn from_offset(items: Vec<T>, offset: u64, limit: u32, total: Option<u64>) -> Self {
        let next = if items.len() as u32 == limit {
            Some((offset + limit as u64).to_string())
        } else {
            None
        };
        Self {
            items,
            next_cursor: next,
            total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShadowSummary {
    pub day: u32,
    pub of: u32,
    pub would_spoil: u64,
    pub would_find: u64,
    pub disagreements: u64,
}

/// The morning summary (README "The Core Experience"): honest about whether
/// processing is complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Summary {
    pub as_of: DateTime<Utc>,
    pub caught_up: bool,
    pub since: DateTime<Utc>,
    pub arrived: u64,
    pub dig_count: u32,
    pub finds_open: u64,
    pub unresolved: u64,
    pub filed: u64,
    pub spoil: u64,
    pub contamination: u64,
    pub remaining_to_sift: u64,
    pub total_to_sift: u64,
    pub director_note: Option<String>,
    pub shadow: Option<ShadowSummary>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub projection_pending: u64,
    pub degraded: Vec<String>,
    /// Rule packs the mailbox's mail matches that are not installed and
    /// not declined: "Riddle recognised GitHub mail (412 messages). Turn
    /// its rules on?" Empty when nothing is recognised.
    #[serde(default)]
    pub pack_offers: Vec<PackOffer>,
}

/// A rule pack whose `[detect]` block matches the mail on hand, offered
/// until it is installed or declined. Packs marked `always` are installed
/// when an account's first listing completes and are never offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackOffer {
    pub pack_id: String,
    pub name: String,
    /// The offer as the Summary says it, with the count in it.
    pub reason_in_words: String,
    /// Messages in the detection window the pack's `[detect]` matched.
    pub matched_messages: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalLine {
    pub name: String,
    pub version: u32,
    pub score: f64,
    pub selected: String,
    pub cached: bool,
}

/// The answer to "Why did this go to Spoil?" (§37, §93).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Explanation {
    pub message_id: MessageId,
    pub disposition: Disposition,
    pub mechanism: DecisionMechanism,
    pub mesh_name: Option<String>,
    pub mesh_version: Option<u32>,
    pub matched: Option<String>,
    pub signals: Vec<SignalLine>,
    pub gate: Option<String>,
    pub escalated: bool,
    pub llm_rationale: Option<String>,
    pub trace_id: TraceId,
    pub shadow: bool,
    pub reason: String,
    /// The deciding Mesh's routes in order, replayed against this message:
    /// each condition with its actual value and result, and where the chain
    /// stopped. Empty when no Mesh decided (a gate, a user decision).
    #[serde(default)]
    pub routes: Vec<RouteCheck>,
    /// The message's level: the user's own when they set one, otherwise
    /// the decision's.
    #[serde(default)]
    pub importance: crate::importance::Importance,
    /// True when `importance` is the user's own setting.
    #[serde(default)]
    pub importance_set_by_you: bool,
    /// The one-time sign-in code the message carries, to copy.
    #[serde(default)]
    #[ts(optional)]
    pub verification_code: Option<String>,
    /// The other Meshes that ran, one line each in evaluation order
    /// (`DecisionTrace::earlier_meshes`): "Bills v3: payment_related .62,
    /// needed .80". Empty when no Mesh decided or the trace predates it.
    #[serde(default)]
    pub earlier_meshes: Vec<String>,
}

/// What today's rules would decide for a message, beside what stands
/// (`decide_preview`). Nothing is written: the would-be classification and
/// trace are returned, not stored, so the stored decision and its trace are
/// untouched. A fresh judge call, when the Signals need one, is recorded as
/// usage like any other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecisionPreview {
    pub message_id: MessageId,
    pub dig_id: DigId,
    /// The current classification and its trace; `None` when the message
    /// has not been sifted yet.
    pub stored: Option<Classification>,
    pub stored_trace: Option<DecisionTrace>,
    pub would: Classification,
    pub would_trace: DecisionTrace,
    /// Whether the disposition, the deciding Mesh or the Strata differ.
    pub changed: bool,
    /// The difference in one sentence: "would be Filed by Bills v4 instead
    /// of Spoil by Frequent promotions v2".
    pub diff: String,
    /// Provenance of the preview's Signals: "13 matched on cached Signals,
    /// 1 needed a fresh evaluation".
    pub note: String,
}

/// One route of a Mesh, checked against one message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteCheck {
    /// The `WHEN` condition the route sits under, as a checked clause.
    pub when: Option<ClauseCheck>,
    /// The route's own conditions, split on AND.
    pub clauses: Vec<ClauseCheck>,
    /// What the route decides, e.g. `Find: Review + Stratum: Engineering`.
    pub output: String,
    pub outcome: RouteOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClauseCheck {
    /// The condition as written, e.g. `review_requested@v1 > .80`.
    pub text: String,
    /// The message's value, e.g. `0.16` or `github.com`; `None` when a
    /// Signal was not scored for this message.
    pub actual: Option<String>,
    pub result: ClauseResult,
    /// The Signal the clause reads, as `name@vN`, so a client can tell
    /// which scores the routes used without parsing `text`. `None` for a
    /// fact or list clause, or one that reads several Signals.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub signal: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ClauseResult {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RouteOutcome {
    /// This route decided the message.
    Matched,
    /// Checked and did not match.
    NoMatch,
    /// A Signal it needs was not scored, so the chain stopped here.
    Unknown,
    /// Not checked: an earlier route decided or stopped the chain.
    NotReached,
}

/// Events emitted to the webview. They carry signals, not volume: a client
/// that needs the data asks for it (§7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum RiddleEvent {
    JobChanged {
        job: Job,
    },
    JobProgress {
        job_id: JobId,
        done: u64,
        total: Option<u64>,
        step: String,
    },
    DigStatusChanged {
        dig_id: DigId,
        status: DigStatus,
    },
    FindCreated {
        find: Find,
    },
    FindChanged {
        find_id: FindId,
    },
    ActivityAppended {
        activity: Activity,
    },
    MessageClassified {
        message_id: MessageId,
        dig_id: DigId,
        disposition: Disposition,
        shadow: bool,
    },
    MessagesChanged {
        dig_id: DigId,
        count: u64,
    },
    SummaryChanged,
    StoreFault {
        message: String,
    },
    StoreFaultCleared,
    Notify {
        title: String,
        body: String,
        find_id: Option<FindId>,
    },
    /// A Gmail sign-in (new account or sign in again) finished. Typed, so
    /// the UI waits on the Dig id rather than matching a toast title.
    GmailSignIn {
        dig_id: DigId,
        connected: bool,
        message: Option<String>,
    },
    UpdateAvailable {
        version: String,
    },
    /// Developer overlay: a Sift stage completed for a message (Live Pipeline, §82).
    /// `pass_id` names the Sift pass, as the trace and the log lines do.
    PipelineStage {
        message_id: MessageId,
        pass_id: String,
        stage: String,
        ms: u64,
    },
    /// Director streaming: text as it arrives.
    DirectorDelta {
        conversation_id: crate::ids::ConversationId,
        text: String,
    },
    /// Director invoked an Action.
    DirectorToolCall {
        conversation_id: crate::ids::ConversationId,
        call: crate::conversation::ToolCall,
    },
    /// Director finished a turn (or failed with `error`).
    DirectorDone {
        conversation_id: crate::ids::ConversationId,
        error: Option<String>,
    },
    /// A Director proposal (Mesh, Signal, Never Hidden entry) awaits the user.
    ProposalCreated {
        pending_id: String,
        summary: String,
    },
    /// A proposal left the pending list without the user's hand on it
    /// (a newer one from the same conversation replaced it).
    ProposalResolved {
        pending_id: String,
    },
    ExpeditionChanged {
        expedition_id: crate::ids::ExpeditionId,
    },
    SettingsChanged,
}

impl RiddleEvent {
    pub const NAME: &'static str = "riddle://event";
}

/// One rule's record: how often it decided, how often the user reversed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RulePrecision {
    pub mesh_id: MeshId,
    pub name: String,
    pub version: u32,
    pub status: MeshStatus,
    pub decisions: u64,
    pub reversed: u64,
    pub shadow_proposed: u32,
    pub shadow_agreed: u32,
    /// The demotion rule's verdict, from the same numbers the engine uses.
    pub reversed_often: bool,
}

/// The seducible moment (Flow 7): one account whose last Spoil calls all
/// stood, offered as a reason to let Riddle act there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrustOffer {
    pub dig_id: DigId,
    pub dig_name: String,
    /// Consecutive Sift Spoil decisions on that account without a correction.
    pub spoil_calls: u64,
    pub projection: ProjectionMode,
    pub autonomy: Autonomy,
}

/// Evidence for the trust card: what Riddle decided in the window, how much
/// the user reversed or undid, what it would have archived had it been at
/// Act, and each rule's precision. All of it is computed locally from the
/// stores; none of it is a survey.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrustStats {
    pub since: DateTime<Utc>,
    pub dig_count: u32,
    /// Sift decisions (not the user's own, not Shadow proposals).
    pub decisions: u64,
    /// Corrections the user made in the window that still stand.
    pub corrected: u64,
    /// Activity entries in the window the user undid.
    pub undone: u64,
    /// Spoil decisions held below Act on accounts that would archive.
    pub would_archive: u64,
    pub rules: Vec<RulePrecision>,
    pub offer: Option<TrustOffer>,
}
