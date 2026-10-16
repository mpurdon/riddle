//! Meshes: versioned policy over facts and Signals (§2, §36–§38). The AST is
//! here; the text parser and evaluator live in `riddle-mesh`. Keeping the AST
//! in core lets the UI render a Mesh structurally and the simulator run
//! without the parser.

use crate::action::Autonomy;
use crate::classification::{Disposition, DispositionKind};
use crate::facts::FactValue;
use crate::ids::{DigId, MeshId};
use crate::signal::CreatedBy;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum MeshScope {
    AllDigs,
    DigGroup { group: String },
    Dig { dig_id: DigId },
}

impl MeshScope {
    /// Specificity for resolution order: Dig beats group beats all.
    pub fn specificity(&self) -> u8 {
        match self {
            MeshScope::AllDigs => 0,
            MeshScope::DigGroup { .. } => 1,
            MeshScope::Dig { .. } => 2,
        }
    }
    pub fn applies_to(&self, dig_id: &DigId, group: Option<&str>) -> bool {
        match self {
            MeshScope::AllDigs => true,
            MeshScope::DigGroup { group: g } => group == Some(g.as_str()),
            MeshScope::Dig { dig_id: d } => d == dig_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CmpOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
    /// Text contains, case-insensitive: `subject CONTAINS "dependabot"`.
    Contains,
}

impl CmpOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Gt => ">",
            CmpOp::Gte => ">=",
            CmpOp::Lt => "<",
            CmpOp::Lte => "<=",
            CmpOp::Eq => "=",
            CmpOp::Neq => "!=",
            CmpOp::Contains => "CONTAINS",
        }
    }
    pub fn holds_f64(self, left: f64, right: f64) -> bool {
        match self {
            CmpOp::Gt => left > right,
            CmpOp::Gte => left >= right,
            CmpOp::Lt => left < right,
            CmpOp::Lte => left <= right,
            CmpOp::Eq => (left - right).abs() < f64::EPSILON,
            CmpOp::Neq => (left - right).abs() >= f64::EPSILON,
            CmpOp::Contains => false,
        }
    }
}

/// The named lists a Mesh may test membership against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NamedList {
    NeverHidden,
    Contacts,
    Vips,
    Organisation,
    Family,
}

impl NamedList {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "never_hidden" => NamedList::NeverHidden,
            "contacts" => NamedList::Contacts,
            "vips" => NamedList::Vips,
            "organisation" | "organization" => NamedList::Organisation,
            "family" => NamedList::Family,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            NamedList::NeverHidden => "never_hidden",
            NamedList::Contacts => "contacts",
            NamedList::Vips => "vips",
            NamedList::Organisation => "organisation",
            NamedList::Family => "family",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Expr {
    /// `review_requested@v3 > .80`, optionally `signal@v3:CHOICE > .5`.
    Signal {
        signal: String,
        version: u32,
        choice: Option<String>,
        op: CmpOp,
        threshold: f64,
    },
    /// `thread_length > 3`, `gmail_category = promotions`, `mailing_list` (bool shorthand).
    Fact {
        fact: String,
        op: CmpOp,
        value: FactValue,
    },
    /// `sender IN never_hidden`.
    InList {
        subject: String,
        list: NamedList,
    },
    /// `sender_domain IN list:banks`: a list the user (or a pack's
    /// questions) filled in (`crate::lists::NamedLists`).
    InNamedList {
        subject: String,
        list: String,
    },
    And {
        items: Vec<Expr>,
    },
    Or {
        items: Vec<Expr>,
    },
    Not {
        item: Box<Expr>,
    },
    /// `ELSE`.
    Always,
}

impl Expr {
    /// Every `signal@version` this expression reads; Signal Selection is the
    /// union over the Meshes in scope (§25).
    pub fn signal_refs(&self, out: &mut Vec<(String, u32)>) {
        match self {
            Expr::Signal {
                signal, version, ..
            } => {
                if !out.iter().any(|(s, v)| s == signal && v == version) {
                    out.push((signal.clone(), *version));
                }
            }
            Expr::And { items } | Expr::Or { items } => {
                items.iter().for_each(|e| e.signal_refs(out))
            }
            Expr::Not { item } => item.signal_refs(out),
            Expr::Fact { .. } | Expr::InList { .. } | Expr::InNamedList { .. } | Expr::Always => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MeshOutput {
    pub disposition: Option<Disposition>,
    pub strata: Vec<String>,
    pub notify: bool,
    pub escalate: bool,
    /// `+ Replaces`: the Find this route raises closes the older open Finds
    /// it raised for the same sender, so a daily reminder is one Find, not a
    /// week of them. Only meaningful with a Find disposition.
    #[serde(default)]
    pub replaces: bool,
    /// `+ Expires:`: when the Find this route raises stops mattering, so a
    /// reminder nobody acted on does not linger. Only meaningful with a Find.
    #[serde(default)]
    #[ts(optional)]
    pub expires: Option<Expiry>,
    /// `+ Importance: high`: how much the message matters, instead of the
    /// level Sift would derive from the decision.
    #[serde(default)]
    #[ts(optional)]
    pub importance: Option<crate::importance::Importance>,
}

impl MeshOutput {
    /// A route that only adds Strata (`→ Stratum: Finance`) is a tag: it
    /// says what the message is about, decides nothing, and evaluation goes
    /// on to the route that does. Topic is not who sent it, so a tag can add
    /// Finance to mail Human correspondence files under People.
    pub fn is_tag(&self) -> bool {
        self.disposition.is_none()
            && !self.escalate
            && !self.notify
            && self.importance.is_none()
            && !self.strata.is_empty()
    }
}

/// The local hour `Expires: next morning` means.
pub const NEXT_MORNING_HOUR: u32 = 9;

/// When a Find stops mattering. The local forms are resolved in the user's
/// timezone when Sift raises the Find, daylight saving included, because a
/// fixed span ("18h") ends at a different hour depending on when the
/// message came.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Expiry {
    /// `Expires: 18h`, `Expires: 2d`: that long after the message's date.
    After { hours: u32 },
    /// `Expires: 15m`: minutes, for what is useful only briefly (a
    /// sign-in code). Under an hour; an hour or more is `After`.
    AfterMinutes { minutes: u32 },
    /// `Expires: end of day`: midnight at the end of the day it arrived.
    EndOfDay,
    /// `Expires: next morning`: `NEXT_MORNING_HOUR` on the day after it
    /// arrived, so one that comes late in the evening still gets a morning.
    NextMorning,
    /// `Expires: after its dates`: the Find waits, snoozed, until
    /// `DATED_DAY_HOUR` on the first day the message is about and expires
    /// at the end of its last (`Facts::dated`). A parking booking made in June was a
    /// Pay Find among the open ones for months, and a hotel confirmation
    /// was still open a week after the stay (15 Oct 2026). Without booking
    /// dates the Find neither waits nor expires.
    AfterDates,
}

/// The local hour a dated Find wakes on its first day: early enough
/// for a morning flight's check-in, late enough not to arrive overnight.
pub const DATED_DAY_HOUR: u32 = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Route {
    /// What the evaluator tests. For a route inside a `WHEN` block this is
    /// the block's condition AND the route's own, so evaluation, simulation
    /// and explanation need no notion of blocks.
    pub condition: Expr,
    pub output: MeshOutput,
    /// The source line, for the Mesh Inspector and the "Why?" answer.
    pub text: String,
    /// The `WHEN` condition this route sits under, if any; kept so the text
    /// formats back as the block the user wrote.
    #[serde(default)]
    pub when: Option<Expr>,
    /// The route's own condition inside its block; `None` is the block's
    /// `ELSE`, which matches whatever the block condition matches.
    #[serde(default)]
    pub local: Option<Expr>,
    /// The route as a sentence ("When the sender's domain is github.com and
    /// review requested is very likely, it is something to review under
    /// Engineering."), written by `riddle_mesh::humanize` when parsed.
    #[serde(default)]
    pub words: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Guard {
    pub never: DispositionKind,
    pub condition: Expr,
    pub text: String,
    /// The guard as a sentence ("Never Spoil when the sender is on your
    /// Never Hidden list.").
    #[serde(default)]
    pub words: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MeshStatus {
    /// Proposed, not yet running.
    Draft,
    /// Running in SUGGEST: proposals recorded, nothing applied.
    Shadow,
    Active,
    Disabled,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Mesh {
    pub id: MeshId,
    pub name: String,
    pub version: u32,
    pub scope: MeshScope,
    /// Lower runs first within one scope.
    pub priority: i32,
    pub routes: Vec<Route>,
    pub guards: Vec<Guard>,
    pub status: MeshStatus,
    /// The Mesh text as authored; the AST is derived from it.
    pub source_text: String,
    pub created_at: DateTime<Utc>,
    pub created_by: CreatedBy,
    pub change_note: Option<String>,
    pub shadow_started_at: Option<DateTime<Utc>>,
    /// Agreement observed during the Shadow Period: proposals the user did
    /// not contradict over proposals made.
    pub shadow_agreed: u32,
    pub shadow_proposed: u32,
    pub deleted_at: Option<DateTime<Utc>>,
    /// The most this Mesh's decisions may do in the mailbox, whatever the
    /// account allows: `Observe` or `Suggest` keep them inside Riddle, `Act`
    /// lets projection write them. `None` follows the account. Written as
    /// `Autonomy:` in the header (Flow 7, per-rule autonomy).
    #[serde(default)]
    #[ts(optional)]
    pub autonomy: Option<Autonomy>,
    /// Set when reversals sent this version back to Shadow; cleared when
    /// the user changes the status themselves. The Meshes page reads it
    /// instead of guessing from the status and the start date.
    #[serde(default)]
    #[ts(optional)]
    pub demoted: Option<Demotion>,
    /// A catch-all for mail no specific Mesh decided ("Updates", "Automated
    /// notifications"). It changes nothing about how mail is decided: the
    /// run order still decides. It says that losing mail to earlier Meshes
    /// is this Mesh's job, so the run order and previews stop flagging it
    /// and keep their warnings for specific rules that compete.
    #[serde(default)]
    pub fallback: bool,
    /// The rule pack this Mesh came from, if any (`riddle_mesh::packs`).
    #[serde(default)]
    #[ts(optional)]
    pub pack: Option<PackRef>,
}

/// One running Mesh in the spot check: what it decided and how its checked
/// decisions were graded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuleCheckRow {
    pub mesh_id: MeshId,
    pub mesh_name: String,
    pub priority: i32,
    pub status: MeshStatus,
    pub fallback: bool,
    /// Current decisions it made, on enabled accounts.
    pub decided: u64,
    pub right: u64,
    pub wrong: u64,
}

/// One decision to grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuleSample {
    pub dig_id: DigId,
    pub message_id: crate::ids::MessageId,
    pub from: crate::mail::Address,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub disposition: crate::classification::Disposition,
    pub strata: Vec<String>,
    /// `right` or `wrong` when already graded.
    pub grade: Option<String>,
}

/// Which pack, and which version of it, a Mesh was installed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackRef {
    pub id: String,
    pub version: u32,
}

/// The demotion rule (Flow 7): an Active Mesh version steps back to Shadow
/// once it has this many decisions on an account and the user reversed a
/// quarter or more. Sift applies it; the trust card reports it.
pub const DEMOTE_MIN_DECISIONS: usize = 20;

/// The rule's verdict for a count of decisions and reversals.
pub fn reversed_often(decided: usize, reversed: usize) -> bool {
    decided >= DEMOTE_MIN_DECISIONS && reversed * 4 >= decided
}

/// Why and when a Mesh stepped back to Shadow on evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Demotion {
    pub at: DateTime<Utc>,
    pub reversed: u32,
    pub decided: u32,
}

impl Mesh {
    pub fn key(&self) -> String {
        format!("{} v{}", self.name, self.version)
    }
    pub fn signal_refs(&self) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        for r in &self.routes {
            r.condition.signal_refs(&mut out);
        }
        for g in &self.guards {
            g.condition.signal_refs(&mut out);
        }
        out
    }
}

/// The simulation window from an Action's or command's `windowDays`
/// argument, clamped to a year. Three call sites parsed it three ways
/// (one default, two defaults, one without the clamp); each keeps its own
/// default and shares the parse.
pub fn window_days(args: &serde_json::Value, default: u32) -> u32 {
    args.get("windowDays")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(365) as u32)
        .unwrap_or(default)
        .clamp(1, 365)
}

/// One historical message's before and after in a simulation (§38).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Transition {
    pub from: DispositionKind,
    pub to: DispositionKind,
    pub count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SimulationReport {
    pub mesh_id: Option<MeshId>,
    pub mesh_name: String,
    pub from_version: Option<u32>,
    pub to_version: u32,
    pub historical_messages: u64,
    pub unchanged: u64,
    pub changed: u64,
    pub transitions: Vec<Transition>,
    /// Messages whose Signals are not cached and would need evaluation.
    pub new_judge_calls_required: u64,
    pub estimated_cost_usd: Option<f64>,
    /// A sample of changed message ids for inspection.
    pub sample_changed: Vec<crate::ids::MessageId>,
    pub window_days: u32,
    /// Messages whose disposition stays but that a `+ Replaces` route would
    /// put in its series; applying re-decides them too, so a store's older
    /// sales, already Spoil, are superseded by its newest.
    #[serde(default)]
    pub joins_series: u64,
    /// Messages that keep their disposition but would be recorded
    /// differently: another Find type, other Strata, Notify, or leaving a
    /// `+ Replaces` series. Counted in `unchanged`, and re-decided by apply.
    #[serde(default)]
    pub recorded_changes: u64,
    /// Messages never scored on a Signal the candidate reads; applying sends
    /// them back to Sift to be scored (a judge call each, counted in
    /// `new_judge_calls_required` and its cost).
    #[serde(default)]
    pub rescore: u64,
    /// Messages whose outcome this edit changes: the candidate against
    /// today's rules re-run unchanged, not against the stored decisions.
    /// `changed` counted both, and on a real mailbox 16 of a preview's 20
    /// "changes" happened with the Mesh text untouched (rules and Signals
    /// had moved since the mail was decided); the Director read the total
    /// as its edit's effect (12 Oct 2026).
    #[serde(default)]
    pub edit_changed: u64,
    /// `edit_changed` by from → to, where "from" is what today's rules decide.
    #[serde(default)]
    pub edit_transitions: Vec<Transition>,
    /// A sample of the messages the edit changes.
    #[serde(default)]
    pub sample_edit_changed: Vec<crate::ids::MessageId>,
    /// Messages today's rules, unchanged, would decide differently from how
    /// they are stored: re-decided by apply-to-history whatever the edit.
    #[serde(default)]
    pub drift: u64,
    /// Running Meshes that decide mail this one matches, because they run
    /// first: "15 of the messages this Mesh matches are taken first by
    /// Security and account alerts". Most first.
    #[serde(default)]
    pub taken_first_by: Vec<MeshOverlap>,
    /// Running Meshes whose mail this one would take: messages it decides
    /// that would otherwise be decided by them. Most first.
    #[serde(default)]
    pub takes_from: Vec<MeshOverlap>,
}

/// Another Mesh and how many messages two Meshes both claim. Meshes run in
/// one order (scope, then Priority, then name), and a generic rule early in
/// it can quietly decide mail a specific rule later on was written for;
/// this is how that is seen before it bites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MeshOverlap {
    pub mesh_id: MeshId,
    pub mesh_name: String,
    pub priority: i32,
    pub count: u64,
}

/// Every running Mesh in the order it runs, with what it decided over a
/// window and which earlier Meshes took mail it also matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MeshOrder {
    pub window_days: u32,
    /// Received messages in the window that the rules were run over.
    pub messages: u64,
    pub rows: Vec<MeshOrderRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MeshOrderRow {
    /// 1 runs first.
    pub position: u32,
    pub mesh_id: MeshId,
    pub mesh_name: String,
    pub version: u32,
    pub status: MeshStatus,
    pub scope: MeshScope,
    pub priority: i32,
    /// A catch-all (`Mesh::fallback`): mail taken first is expected.
    pub fallback: bool,
    /// Messages this Mesh decides under the current rules.
    pub decided: u64,
    /// Messages it matches that an earlier Mesh decides instead.
    pub taken_first_by: Vec<MeshOverlap>,
}
