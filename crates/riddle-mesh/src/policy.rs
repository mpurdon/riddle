//! Sift's confidence policy: how sure a Signal-decided route must be before
//! it stands. Shared by the evaluator (which lets a too-close match fall
//! through to a later Mesh), Sift (which holds what stands nowhere) and the
//! simulator (which predicts both), so the three cannot disagree.

use riddle_core::DispositionKind;

/// What Sift requires of a Signal-decided route before it stands: the
/// weakest Signal's margin over its threshold, per disposition, and whether
/// mail not in English is held for escalation. Sift's own defaults come from
/// here, so the simulation and the decision cannot drift apart. Before the
/// simulation applied it, a candidate previewed as "1,289 to Spoil" on a
/// mailbox where Sift would have left every one of them Unresolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfidencePolicy {
    pub margin_spoil: f64,
    pub margin_find: f64,
    pub margin_filed: f64,
    pub margin_contamination: f64,
    pub escalate_non_english: bool,
}

impl Default for ConfidencePolicy {
    fn default() -> Self {
        Self {
            margin_spoil: 0.15,
            margin_find: 0.05,
            margin_filed: 0.05,
            margin_contamination: 0.25,
            escalate_non_english: true,
        }
    }
}

impl ConfidencePolicy {
    /// The policy as a trace stores it, so a decision read after the
    /// thresholds changed still says what it was held to.
    pub fn thresholds(&self) -> riddle_core::PolicyThresholds {
        riddle_core::PolicyThresholds {
            margin_find: self.margin_find,
            margin_spoil: self.margin_spoil,
            margin_filed: self.margin_filed,
            margin_contamination: self.margin_contamination,
            escalate_non_english: self.escalate_non_english,
        }
    }

    pub fn required_margin(&self, kind: DispositionKind) -> f64 {
        match kind {
            DispositionKind::Spoil => self.margin_spoil,
            DispositionKind::Find => self.margin_find,
            DispositionKind::Filed => self.margin_filed,
            DispositionKind::Contamination => self.margin_contamination,
            DispositionKind::Unresolved => 0.0,
        }
    }

    /// Why Sift would hold a route to `kind` that needed a Signal (`margin`)
    /// for a message in `language`, or `None` when the route stands. A
    /// fact-only route has no margin and always stands; a route to
    /// Unresolved asks for nothing. The texts are what Sift records as the
    /// escalation reason, so the simulation and the decision say the same
    /// thing for the same message.
    /// How far a route's weakest Signal margin falls short of what `kind`
    /// requires, as (margin, needed), or `None` when it is enough. The one
    /// definition of "too close to call": the hold reason and the
    /// evaluator's fall-through both read it, so they cannot disagree.
    pub fn shortfall(&self, kind: DispositionKind, margin: Option<f64>) -> Option<(f64, f64)> {
        let m = margin?;
        if kind == DispositionKind::Unresolved {
            return None;
        }
        let needed = self.required_margin(kind);
        (m < needed).then_some((m, needed))
    }

    /// Why Sift would hold a route to `kind` that needed a Signal (`margin`)
    /// for a message in `language`, or `None` when the route stands. A
    /// fact-only route has no margin and always stands; a route to
    /// Unresolved asks for nothing. The texts are what Sift records as the
    /// escalation reason, so the simulation and the decision say the same
    /// thing for the same message.
    pub fn hold_reason(
        &self,
        kind: DispositionKind,
        margin: Option<f64>,
        language: &str,
    ) -> Option<String> {
        if let Some((m, needed)) = self.shortfall(kind, margin) {
            return Some(format!(
                "weakest Signal margin {} for {}",
                shortfall_words(m, needed),
                kind.as_str()
            ));
        }
        margin?;
        if kind == DispositionKind::Unresolved {
            return None;
        }
        // An empty language is unknown, like "und", not foreign.
        let foreign = !language.is_empty() && language != "en" && language != "und";
        if self.escalate_non_english && foreign {
            return Some("the message is not in English; Signals are less reliable".into());
        }
        None
    }

    /// Whether Sift would stand by a route that needed a Signal (`margin`)
    /// for a message in `language`.
    pub fn stands(&self, kind: DispositionKind, margin: Option<f64>, language: &str) -> bool {
        self.hold_reason(kind, margin, language).is_none()
    }
}

/// "0.08 is below the 0.15 required", or "0.149 is just under the 0.15
/// required" when two places would print the same number: "0.15 is below
/// the 0.15 required" read as a contradiction.
pub fn shortfall_words(margin: f64, needed: f64) -> String {
    if format!("{margin:.2}") == format!("{needed:.2}") {
        format!("{margin:.3} is just under the {needed:.2} required")
    } else {
        format!("{margin:.2} is below the {needed:.2} required")
    }
}
