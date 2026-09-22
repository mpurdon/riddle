//! Privacy boundaries (`ARCHITECTURE.md` §57): what may leave the device, per
//! Dig, and the redaction applied to Context before it does.
//!
//! Three pieces live here because they are policy, not plumbing:
//!
//! - `allowed(mode, purpose)` is the one table that says which cloud calls a
//!   `PrivacyMode` permits. Every AI boundary (judge, LLM, embeddings) asks it
//!   before touching the network, so a Dig marked local-only cannot leak
//!   through a forgotten code path.
//! - `redact` replaces addresses, phone numbers and capitalised name pairs
//!   with stable placeholders and returns the map so the UI can show the
//!   original again. The placeholders are stable within one call so the model
//!   can still reason about "[person-1] asked [person-2]".
//! - `disclosure` is the one-sentence statement the app shows: what leaves
//!   the device, to whom, and why, plus the Limited Use sentence Google
//!   requires of Gmail API clients.
//!
//! Redaction is a courtesy to the vendor boundary, not a security control:
//! the model still sees the message body. The security controls are in
//! `riddle-agent` (untrusted marking, egress gating).

pub mod redact;

use riddle_core::PrivacyMode;

pub use redact::{redact, RedactionMap};

/// Why a cloud call is being made. The table in `allowed` is keyed on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// Bounded semantic judgment (Jev or a replacement, §18).
    Judge,
    /// Open-ended reasoning: escalation, Director, Expeditions (§39, §40).
    Llm,
    /// Vector embeddings for semantic search.
    Embeddings,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Judge => "judge",
            Purpose::Llm => "llm",
            Purpose::Embeddings => "embeddings",
        }
    }
}

/// Whether a `PrivacyMode` permits a call for `purpose`.
///
/// Embeddings are always built locally (§45), so they are permitted in every
/// mode; the local-only mode permits nothing else.
pub fn allowed(mode: PrivacyMode, purpose: Purpose) -> bool {
    match (mode, purpose) {
        (_, Purpose::Embeddings) => true,
        (PrivacyMode::LocalOnly, _) => false,
        (PrivacyMode::CloudJudgment, Purpose::Judge) => true,
        (PrivacyMode::CloudJudgment, Purpose::Llm) => false,
        (PrivacyMode::CloudReasoning, _) => true,
    }
}

/// The most restrictive of several modes, for a task that spans Digs.
pub fn most_restrictive(modes: impl IntoIterator<Item = PrivacyMode>) -> PrivacyMode {
    let mut result = PrivacyMode::CloudReasoning;
    for m in modes {
        result = match (result, m) {
            (PrivacyMode::LocalOnly, _) | (_, PrivacyMode::LocalOnly) => PrivacyMode::LocalOnly,
            (PrivacyMode::CloudJudgment, _) | (_, PrivacyMode::CloudJudgment) => {
                PrivacyMode::CloudJudgment
            }
            _ => PrivacyMode::CloudReasoning,
        };
    }
    result
}

/// What the disclosure sentence needs to know. Mirrors the settings shape
/// without depending on the settings crate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisclosureInput {
    /// e.g. "TypeSafe AI (Jev)"; `None` when no judge is configured.
    pub judge_vendor: Option<String>,
    /// e.g. "Amazon Bedrock (Anthropic Claude)"; `None` when no LLM is configured.
    pub llm_vendor: Option<String>,
    /// (Dig name, mode) for every Dig, or the one Dig asked about.
    pub digs: Vec<(String, PrivacyMode)>,
}

/// The sentence Google requires Gmail API clients to display.
pub const LIMITED_USE: &str = "Riddle's use and transfer of information received from Google APIs \
     adheres to the Google API Services User Data Policy, including the Limited Use requirements.";

/// The one-sentence "what leaves the device" statement plus the Limited Use
/// sentence (§57). Vendors are named; nothing is used for training.
pub fn disclosure(input: &DisclosureInput) -> String {
    let judge = input
        .judge_vendor
        .as_deref()
        .unwrap_or("no judgment service");
    let llm = input
        .llm_vendor
        .as_deref()
        .unwrap_or("no reasoning service");

    let mut local = Vec::new();
    let mut judgment = Vec::new();
    let mut reasoning = Vec::new();
    for (name, mode) in &input.digs {
        match mode {
            PrivacyMode::LocalOnly => local.push(name.as_str()),
            PrivacyMode::CloudJudgment => judgment.push(name.as_str()),
            PrivacyMode::CloudReasoning => reasoning.push(name.as_str()),
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if !reasoning.is_empty() {
        parts.push(format!(
            "For {}, normalized message text is sent to {judge} for classification and redacted \
             Context is sent to {llm} for reasoning",
            list(&reasoning)
        ));
    }
    if !judgment.is_empty() {
        parts.push(format!(
            "For {}, only normalized message text is sent to {judge} for classification",
            list(&judgment)
        ));
    }
    if !local.is_empty() {
        parts.push(format!("For {}, nothing leaves this device", list(&local)));
    }
    let body = if parts.is_empty() {
        "No Dig is connected, so nothing leaves this device".to_string()
    } else {
        parts.join("; ")
    };
    format!("{body}; no vendor trains on your mail. {LIMITED_USE}")
}

fn list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [a, b] => format!("{a} and {b}"),
        many => {
            let (last, rest) = many.split_last().unwrap_or((&"", &[]));
            format!("{} and {last}", rest.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_permits_nothing_but_local_embeddings() {
        assert!(!allowed(PrivacyMode::LocalOnly, Purpose::Judge));
        assert!(!allowed(PrivacyMode::LocalOnly, Purpose::Llm));
        assert!(allowed(PrivacyMode::LocalOnly, Purpose::Embeddings));
    }

    #[test]
    fn cloud_judgment_permits_the_judge_but_not_the_llm() {
        assert!(allowed(PrivacyMode::CloudJudgment, Purpose::Judge));
        assert!(!allowed(PrivacyMode::CloudJudgment, Purpose::Llm));
        assert!(allowed(PrivacyMode::CloudReasoning, Purpose::Llm));
    }

    #[test]
    fn the_most_restrictive_mode_wins_across_digs() {
        assert_eq!(
            most_restrictive([PrivacyMode::CloudReasoning, PrivacyMode::CloudJudgment]),
            PrivacyMode::CloudJudgment
        );
        assert_eq!(
            most_restrictive([PrivacyMode::CloudReasoning, PrivacyMode::LocalOnly]),
            PrivacyMode::LocalOnly
        );
        assert_eq!(most_restrictive([]), PrivacyMode::CloudReasoning);
    }

    #[test]
    fn the_disclosure_names_vendors_per_dig_and_carries_the_limited_use_sentence() {
        let s = disclosure(&DisclosureInput {
            judge_vendor: Some("TypeSafe AI (Jev)".into()),
            llm_vendor: Some("Amazon Bedrock (Anthropic Claude)".into()),
            digs: vec![
                ("Personal".into(), PrivacyMode::CloudReasoning),
                ("Work".into(), PrivacyMode::LocalOnly),
            ],
        });
        assert!(s.contains("For Personal, normalized message text is sent to TypeSafe AI (Jev)"));
        assert!(s.contains("Amazon Bedrock (Anthropic Claude) for reasoning"));
        assert!(s.contains("For Work, nothing leaves this device"));
        assert!(s.contains("no vendor trains on your mail"));
        assert!(s.contains(LIMITED_USE));
    }

    #[test]
    fn the_disclosure_without_digs_says_nothing_leaves() {
        let s = disclosure(&DisclosureInput::default());
        assert!(s.starts_with("No Dig is connected, so nothing leaves this device"));
    }
}
