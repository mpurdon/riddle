//! Meshes: versioned policy over facts and Signals (§2, §36–§38). The AST is
//! here; the text parser and evaluator live in `riddle-mesh`. Keeping the AST
//! in core lets the UI render a Mesh structurally and the simulator run
//! without the parser.

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
    Signal { signal: String, version: u32, choice: Option<String>, op: CmpOp, threshold: f64 },
    /// `thread_length > 3`, `gmail_category = promotions`, `mailing_list` (bool shorthand).
    Fact { fact: String, op: CmpOp, value: FactValue },
    /// `sender IN never_hidden`.
    InList { subject: String, list: NamedList },
    And { items: Vec<Expr> },
    Or { items: Vec<Expr> },
    Not { item: Box<Expr> },
    /// `ELSE`.
    Always,
}

impl Expr {
    /// Every `signal@version` this expression reads; Signal Selection is the
    /// union over the Meshes in scope (§25).
    pub fn signal_refs(&self, out: &mut Vec<(String, u32)>) {
        match self {
            Expr::Signal { signal, version, .. } => {
                if !out.iter().any(|(s, v)| s == signal && v == version) {
                    out.push((signal.clone(), *version));
                }
            }
            Expr::And { items } | Expr::Or { items } => items.iter().for_each(|e| e.signal_refs(out)),
            Expr::Not { item } => item.signal_refs(out),
            Expr::Fact { .. } | Expr::InList { .. } | Expr::Always => {}
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Route {
    pub condition: Expr,
    pub output: MeshOutput,
    /// The source line, for the Mesh Inspector and the "Why?" answer.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Guard {
    pub never: DispositionKind,
    pub condition: Expr,
    pub text: String,
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
}

impl Mesh {
    pub fn key(&self) -> String { format!("{} v{}", self.name, self.version) }
    pub fn signal_refs(&self) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        for r in &self.routes { r.condition.signal_refs(&mut out); }
        for g in &self.guards { g.condition.signal_refs(&mut out); }
        out
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
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
}
