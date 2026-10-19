//! Dispositions, classifications and decision traces: what Riddle decided
//! about a message and everything that participated (`ARCHITECTURE.md` §2,
//! §49, §60). A classification without a trace is a violation of §92.

use crate::find::FindType;
use crate::ids::{ClassificationId, DigId, MeshId, MessageId, TraceId};
use crate::signal::SignalEvaluation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Every message has exactly one disposition, and Unresolved is one of them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Disposition {
    Find { find_type: FindType },
    Filed,
    Spoil,
    Contamination,
    Unresolved,
}

impl Disposition {
    pub fn kind(&self) -> DispositionKind {
        match self {
            Disposition::Find { .. } => DispositionKind::Find,
            Disposition::Filed => DispositionKind::Filed,
            Disposition::Spoil => DispositionKind::Spoil,
            Disposition::Contamination => DispositionKind::Contamination,
            Disposition::Unresolved => DispositionKind::Unresolved,
        }
    }
    /// Whether this disposition removes the message from the user's view.
    pub fn hides(&self) -> bool {
        matches!(
            self,
            Disposition::Spoil | Disposition::Contamination | Disposition::Filed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DispositionKind {
    Find,
    Filed,
    Spoil,
    Contamination,
    Unresolved,
}

impl DispositionKind {
    pub const ALL: [DispositionKind; 5] = [
        DispositionKind::Find,
        DispositionKind::Filed,
        DispositionKind::Spoil,
        DispositionKind::Contamination,
        DispositionKind::Unresolved,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            DispositionKind::Find => "find",
            DispositionKind::Filed => "filed",
            DispositionKind::Spoil => "spoil",
            DispositionKind::Contamination => "contamination",
            DispositionKind::Unresolved => "unresolved",
        }
    }
}

/// Which system made a decision (§49).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DecisionMechanism {
    Deterministic,
    Gate,
    Jev,
    Mesh,
    Llm,
    Temporal,
    Director,
    User,
    Provider,
}

/// The current classification of one message. Re-classification writes a new
/// row and supersedes the old one; history is kept for replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Classification {
    pub id: ClassificationId,
    pub message_id: MessageId,
    pub dig_id: DigId,
    pub disposition: Disposition,
    pub strata: Vec<String>,
    pub notify: bool,
    pub mechanism: DecisionMechanism,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    /// The route or gate that matched, as text, for the "Why?" answer.
    pub matched_rule: Option<String>,
    pub confidence: Option<f64>,
    pub reason: String,
    pub trace_id: TraceId,
    /// True during the Shadow Period: recorded, shown, not applied (§51).
    pub shadow: bool,
    pub created_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    /// The `+ Replaces` series this decision belongs to (Mesh and sender):
    /// only the newest message of a series keeps its standing; the older
    /// ones are superseded by it.
    #[serde(default)]
    #[ts(optional)]
    pub series: Option<String>,
    /// How much the message matters, as the rule or Sift decided it. The
    /// user's own level for the message lives beside it, not here, so a
    /// rule re-deciding the message never overwrites it.
    #[serde(default)]
    pub importance: crate::importance::Importance,
}

/// Per-stage timing for one message, in milliseconds (§68).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TraceTiming {
    pub sync_ms: u64,
    pub facts_ms: u64,
    pub judge_ms: u64,
    pub mesh_ms: u64,
    pub llm_ms: u64,
    pub persist_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TraceCost {
    pub judge_input_tokens: u64,
    pub judge_questions: u32,
    /// USD; `None` means unpriced, never free (§56).
    pub judge_cost_usd: Option<f64>,
    pub llm_input_tokens: u64,
    pub llm_output_tokens: u64,
    pub llm_cost_usd: Option<f64>,
}

/// What the LLM escalation returned (§39). It refines Signals and gives a
/// rationale; it never returns a disposition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EscalationRecord {
    pub reason: String,
    pub model_id: String,
    pub refined: Vec<SignalEvaluation>,
    pub rationale: String,
    pub latency_ms: u64,
}

/// One list clause as it read when the decision was made. The walk used to
/// replay the pinned Mesh version against today's lists, so a sender added
/// to a list afterwards made a stored decision's walk say its route had
/// matched when it had not (17 Oct 2026). The trace now keeps what each
/// list clause saw, and the walk reads that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListClauseResult {
    /// The list as the rule names it: `never_hidden`, `contacts`, or
    /// `list:banks` for one of the user's named lists.
    pub list: String,
    /// The value tested against it: the sender, or another fact's value.
    pub value: String,
    pub matched: bool,
}

/// The confidence policy in force when the decision was made: the margin
/// the weakest Signal of a route must clear, per disposition. Stored so a
/// trace read after the policy changed still says what "too close to call"
/// meant for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PolicyThresholds {
    pub margin_find: f64,
    pub margin_spoil: f64,
    pub margin_filed: f64,
    pub margin_contamination: f64,
    pub escalate_non_english: bool,
}

/// What became of one Mesh when the message was decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum MeshOutcomeKind {
    /// Its route decided the message.
    Matched { route: String },
    /// Every route was checked and none matched.
    NoRouteMatched,
    /// A route matched on a Signal too close to call, so a later Mesh got
    /// its turn: the weakest Signal's score, what it would have needed,
    /// and the margin numbers as the policy states them.
    MarginShortfall {
        route: String,
        /// `name@vN`, when one Signal's distance is the route's margin.
        signal: Option<String>,
        score: Option<f64>,
        needed_score: Option<f64>,
        margin: f64,
        needed: f64,
    },
    /// A condition read a Signal that was not scored for this message, so
    /// the rest of the chain could not be trusted.
    Unknown { signals: Vec<String> },
    /// A route to Spoil or Contamination matched, but an earlier Mesh's
    /// borderline Find must not be hidden by it, so it was set aside.
    HeldForFind { route: String },
    /// Not checked: an earlier Mesh decided.
    NotReached,
}

/// One Mesh's outcome, in the order the evaluator ran them (most specific
/// scope first, then Priority, then name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MeshOutcome {
    pub mesh_id: MeshId,
    pub version: u32,
    pub name: String,
    pub outcome: MeshOutcomeKind,
}

/// `0.62` as the rule syntax writes it: `.62`.
fn two_places(n: f64) -> String {
    let s = format!("{n:.2}");
    s.strip_prefix('0').map(str::to_string).unwrap_or(s)
}

impl MeshOutcome {
    pub fn key(&self) -> String {
        format!("{} v{}", self.name, self.version)
    }

    /// The outcome in one line, for the "Why?" answer's collapsed list of
    /// the other Meshes that ran: `Bills v3: payment_related .62, needed
    /// .80`. Deterministic text with a fixed vocabulary, never a model.
    pub fn line(&self) -> String {
        let key = self.key();
        match &self.outcome {
            MeshOutcomeKind::Matched { .. } => format!("{key}: decided"),
            MeshOutcomeKind::NoRouteMatched => format!("{key}: no route matched"),
            MeshOutcomeKind::MarginShortfall {
                signal: Some(signal),
                score: Some(score),
                needed_score: Some(needed_score),
                ..
            } => {
                let name = signal.split('@').next().unwrap_or(signal);
                format!(
                    "{key}: {name} {}, needed {}",
                    two_places(*score),
                    two_places(*needed_score)
                )
            }
            MeshOutcomeKind::MarginShortfall { margin, needed, .. } => format!(
                "{key}: margin {}, needed {}",
                two_places(*margin),
                two_places(*needed)
            ),
            MeshOutcomeKind::Unknown { signals } if signals.is_empty() => {
                format!("{key}: a Signal was not scored")
            }
            MeshOutcomeKind::Unknown { signals } => {
                format!("{key}: {} not scored", signals.join(", "))
            }
            MeshOutcomeKind::HeldForFind { .. } => {
                format!("{key}: matched, set aside so an earlier borderline Find is not hidden")
            }
            MeshOutcomeKind::NotReached => format!("{key}: not reached"),
        }
    }
}

/// The structured reasoning artifact behind one classification (§60).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecisionTrace {
    pub id: TraceId,
    pub message_id: MessageId,
    pub dig_id: DigId,
    pub objective: String,
    pub strategy: String,
    pub facts: crate::facts::Facts,
    pub gate: Option<String>,
    pub signals: Vec<SignalEvaluation>,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    pub matched_rule: Option<String>,
    pub guard_vetoes: Vec<String>,
    pub escalation: Option<EscalationRecord>,
    pub conclusion: Disposition,
    pub confidence: Option<f64>,
    pub mechanism: DecisionMechanism,
    pub timing: TraceTiming,
    pub cost: TraceCost,
    pub judge_identity: Option<String>,
    pub app_version: String,
    pub build_stamp: String,
    pub shadow: bool,
    pub created_at: DateTime<Utc>,
    /// The Sift pass that made the decision (`SiftInput::pass_id`), the
    /// same id the pass's log lines and Activity carry; empty for a
    /// decision made outside a pass (an older trace, a preview).
    #[serde(default)]
    pub pass_id: String,
    /// What decided, as of the decision (17 Oct 2026): each list clause the
    /// routes read, the confidence policy in force, the winning route's
    /// weakest Signal margin, and every Mesh's outcome in evaluation
    /// order. Empty on a trace stored before these were kept, and on a
    /// decision no Mesh took part in (a gate, a draft, sent mail).
    #[serde(default)]
    pub list_results: Vec<ListClauseResult>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub policy: Option<PolicyThresholds>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub margin: Option<f64>,
    #[serde(default)]
    pub mesh_outcomes: Vec<MeshOutcome>,
}

impl DecisionTrace {
    /// Every other Mesh that ran, one line each in evaluation order: the
    /// ones that looked first and passed, and the ones that looked after a
    /// borderline match and did not take it. The deciding Mesh and the
    /// ones never reached are left out.
    pub fn earlier_meshes(&self) -> Vec<String> {
        self.mesh_outcomes
            .iter()
            .filter(|o| !matches!(o.outcome, MeshOutcomeKind::NotReached))
            .filter(|o| {
                !(Some(&o.mesh_id) == self.mesh_id.as_ref() && Some(o.version) == self.mesh_version)
            })
            .map(MeshOutcome::line)
            .collect()
    }
}

/// The reason an Unresolved decision gives when the Dig's escalation budget
/// ran out. Unresolved mail with it is decided again once the budget resets
/// (`repo::classifications::stale_unresolved` matches on it), so the
/// wording lives in one place.
pub const BUDGET_EXHAUSTED: &str = "escalation budget exhausted for this Dig";

/// The trace strategy of a decision made while the judge could not answer:
/// no key yet, a blip, an open breaker. Housekeeping sends Unresolved mail
/// decided under it back to Sift once a judge answers
/// (`repo::classifications::stale_unresolved`). A judge that failed on the
/// message itself (`JUDGE_FAILED_STRATEGY`: a state too large to send) is
/// not asked again, since it would fail the same way (19 Oct 2026).
pub const JUDGE_UNAVAILABLE_STRATEGY: &str = "deterministic-only (judge unavailable)";
pub const JUDGE_FAILED_STRATEGY: &str = "deterministic-only (judge failed)";
