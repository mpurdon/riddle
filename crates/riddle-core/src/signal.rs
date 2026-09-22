//! Semantic Signals: managed, versioned domain objects (§22–§27). The
//! definition is the prompt, so the authoring rules in §23 are enforced by
//! `Signal::validate` rather than hoped for.

use crate::ids::{MessageId, SignalId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Only Choice and Noul: Score levels are weakly calibrated (§18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SignalPrimitive {
    Choice,
    Noul,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalChoice {
    pub name: String,
    pub definition: String,
}

/// What the state builder includes for this Signal (§25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalInputs {
    pub message: bool,
    pub thread: bool,
    /// Named Context slices, e.g. "sender_relationship", "open_asks".
    pub context_slices: Vec<String>,
}

impl SignalInputs {
    pub fn message_only() -> Self { Self { message: true, thread: false, context_slices: vec![] } }
    pub fn is_context_dependent(&self) -> bool { self.thread || !self.context_slices.is_empty() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SignalStatus {
    Proposed,
    Candidate,
    Evaluating,
    Rejected,
    Experimental,
    Active,
    Deprecated,
}

impl SignalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SignalStatus::Proposed => "proposed",
            SignalStatus::Candidate => "candidate",
            SignalStatus::Evaluating => "evaluating",
            SignalStatus::Rejected => "rejected",
            SignalStatus::Experimental => "experimental",
            SignalStatus::Active => "active",
            SignalStatus::Deprecated => "deprecated",
        }
    }
    /// Production Sift only reads Active Signals; Experimental ones run in test_signal.
    pub fn is_production(self) -> bool { matches!(self, SignalStatus::Active) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CalibrationMethod {
    Platt,
    Conformal,
    /// Raw scores used as-is; thresholds are operating points, not probabilities.
    None,
}

/// The calibration artifact every Signal version carries (§27).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Calibration {
    pub method: CalibrationMethod,
    pub labelled_count: u32,
    pub engine_identity: String,
    pub calibrated_at: DateTime<Utc>,
    /// Platt parameters (a, b) when method is Platt.
    pub platt_a: Option<f64>,
    pub platt_b: Option<f64>,
    pub held_out_agreement: Option<f64>,
}

impl Calibration {
    pub fn uncalibrated(engine_identity: &str) -> Self {
        Self {
            method: CalibrationMethod::None,
            labelled_count: 0,
            engine_identity: engine_identity.to_string(),
            calibrated_at: Utc::now(),
            platt_a: None,
            platt_b: None,
            held_out_agreement: None,
        }
    }
    /// Map a raw probability to a calibrated one.
    pub fn apply(&self, raw: f64) -> f64 {
        match (self.method, self.platt_a, self.platt_b) {
            (CalibrationMethod::Platt, Some(a), Some(b)) => {
                let r = raw.clamp(1e-6, 1.0 - 1e-6);
                let logit = (r / (1.0 - r)).ln();
                1.0 / (1.0 + (-(a * logit + b)).exp())
            }
            _ => raw,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CreatedBy {
    System,
    User,
    Director,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Signal {
    pub id: SignalId,
    pub name: String,
    pub version: u32,
    pub primitive: SignalPrimitive,
    pub definition: String,
    /// For Choice: the options. For Noul: exactly YES and NO.
    pub choices: Vec<SignalChoice>,
    pub inputs: SignalInputs,
    /// ISO 639-1 codes the Signal is trusted for; empty means English only.
    pub language_scope: Vec<String>,
    pub status: SignalStatus,
    pub calibration: Option<Calibration>,
    pub created_at: DateTime<Utc>,
    pub created_by: CreatedBy,
    /// A note on why this version exists.
    pub change_note: Option<String>,
}

impl Signal {
    pub fn key(&self) -> String { format!("{}@v{}", self.name, self.version) }

    /// The authoring rules of §23, checked before a Signal can be proposed.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();
        let d = self.definition.to_lowercase();
        if !self.name.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()) || self.name.is_empty() {
            problems.push("name must be snake_case ascii".into());
        }
        if self.definition.trim().is_empty() {
            problems.push("definition is empty".into());
        }
        if d.contains(" and/or ") || d.contains("and/or") {
            problems.push("definition uses and/or; one property per Signal".into());
        }
        if d.contains(" not ") || d.starts_with("not ") || d.contains("n't ") || d.contains(" never ") {
            problems.push("definition contains a negation; phrase the positive property".into());
        }
        if d.contains("days ago") || d.contains("more than") || d.contains("less than") || d.contains("at least") {
            problems.push("definition compares dates or counts; those are deterministic facts".into());
        }
        match self.primitive {
            SignalPrimitive::Noul => {
                let names: Vec<&str> = self.choices.iter().map(|c| c.name.as_str()).collect();
                if names != ["YES", "NO"] {
                    problems.push("a noul Signal has exactly the choices YES and NO".into());
                }
            }
            SignalPrimitive::Choice => {
                if self.choices.len() < 2 {
                    problems.push("a choice Signal needs at least two choices".into());
                }
                for c in &self.choices {
                    if c.definition.trim().is_empty() {
                        problems.push(format!("choice {} has no definition", c.name));
                    }
                }
            }
        }
        if problems.is_empty() { Ok(()) } else { Err(problems) }
    }

    pub fn noul(name: &str, definition: &str) -> Self {
        Self {
            id: SignalId::new(),
            name: name.to_string(),
            version: 1,
            primitive: SignalPrimitive::Noul,
            definition: definition.to_string(),
            choices: vec![
                SignalChoice { name: "YES".into(), definition: "The property holds.".into() },
                SignalChoice { name: "NO".into(), definition: "The property does not hold.".into() },
            ],
            inputs: SignalInputs::message_only(),
            language_scope: vec!["en".into()],
            status: SignalStatus::Proposed,
            calibration: None,
            created_at: Utc::now(),
            created_by: CreatedBy::System,
            change_note: None,
        }
    }
}

/// A probability distribution over choice names, summing to about one.
pub type Distribution = BTreeMap<String, f64>;

/// One evaluation of one Signal version on one message (§26). The cache key
/// is (message, signal name and version, engine identity, context hash).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalEvaluation {
    pub message_id: MessageId,
    pub signal_name: String,
    pub signal_version: u32,
    /// e.g. `jev:jev-1.13.0`, `bedrock:us.anthropic.claude-sonnet-5`, `null`.
    pub engine_identity: String,
    pub selected: String,
    /// Raw distribution from the engine.
    #[ts(type = "Record<string, number>")]
    pub distribution: Distribution,
    /// Calibrated distribution; equals raw when the calibration method is None.
    #[ts(type = "Record<string, number>")]
    pub calibrated: Distribution,
    pub confidence: f64,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub cached: bool,
    pub deterministic: bool,
    pub context_hash: Option<String>,
    /// Hash of the prompt/schema the engine saw, for replay.
    pub request_hash: Option<String>,
    pub evaluated_at: DateTime<Utc>,
}

impl SignalEvaluation {
    /// Calibrated probability of a choice (YES by default for noul Signals).
    pub fn score(&self, choice: Option<&str>) -> f64 {
        let c = choice.unwrap_or("YES");
        self.calibrated.get(c).or_else(|| self.distribution.get(c)).copied().unwrap_or(0.0)
    }
    pub fn key(&self) -> String { format!("{}@v{}", self.signal_name, self.signal_version) }
}
