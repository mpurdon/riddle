//! Signal Sets and their state builders (§25). A Set groups Signals that
//! read the same input so one judge call answers all of them, and it owns a
//! state builder with a token budget. The budget is a quality control before
//! it is a cost control: the vendor documents that accuracy falls as
//! irrelevant state grows (§18), so a Set includes only the Context slices
//! its Signals declare, and truncation is deterministic and recorded so a
//! trace can say what the judge did not see.

use riddle_core::{sha256_hex, Facts, JudgeState, Signal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetId {
    GitHub,
    Commerce,
    HumanCorrespondence,
    Calendar,
    Financial,
    Security,
    Baseline,
}

impl SetId {
    /// Selection order: domain Sets first, Baseline last as the catch-all.
    pub const ALL: [SetId; 7] = [
        SetId::GitHub,
        SetId::Commerce,
        SetId::HumanCorrespondence,
        SetId::Calendar,
        SetId::Financial,
        SetId::Security,
        SetId::Baseline,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SetId::GitHub => "github",
            SetId::Commerce => "commerce",
            SetId::HumanCorrespondence => "human_correspondence",
            SetId::Calendar => "calendar",
            SetId::Financial => "financial",
            SetId::Security => "security",
            SetId::Baseline => "baseline",
        }
    }

    /// Membership. A Signal may belong to several Sets; assignment picks the
    /// first selected Set that contains it.
    pub fn members(self) -> &'static [&'static str] {
        match self {
            SetId::GitHub => &[
                "review_requested",
                "ci_failure",
                "deployment_failed",
                "owned_work_failed",
                "personally_relevant",
                "routine",
                "requires_response",
            ],
            SetId::Commerce => &[
                "receipt",
                "invoice",
                "represents_charge",
                "transactional",
                "marketing",
                "travel_reservation",
                "acknowledges_cancellation",
            ],
            SetId::HumanCorrespondence => &[
                "requires_response",
                "requires_action",
                "direct_request",
                "asks_question",
                "makes_commitment",
                "requests_action",
                "personally_relevant",
            ],
            SetId::Calendar => &[
                "meeting_related",
                "acknowledges_cancellation",
                "requires_response",
            ],
            SetId::Financial => &[
                "payment_related",
                "invoice",
                "represents_charge",
                "receipt",
                "account_notification",
            ],
            SetId::Security => &["security_alert", "account_notification"],
            SetId::Baseline => &[
                "routine",
                "novel",
                "marketing",
                "newsletter",
                "personally_relevant",
                "requires_response",
                "security_alert",
            ],
        }
    }

    /// The Set's deterministic precondition: the facts that make it relevant.
    /// Baseline and Security are always relevant.
    pub fn relevant(self, facts: &Facts) -> bool {
        let automated = facts.mailing_list
            || facts.precedence_bulk
            || facts.auto_submitted
            || facts.list_unsubscribe;
        let category = facts.gmail_category.as_deref().unwrap_or("");
        match self {
            SetId::GitHub => {
                facts.sender_domain.ends_with("github.com")
                    || facts.sender_domain.ends_with("gitlab.com")
                    || facts.sender_domain.ends_with("bitbucket.org")
            }
            SetId::Commerce => {
                facts.has_structured_receipt
                    || category == "promotions"
                    || category == "updates"
                    || automated
            }
            SetId::HumanCorrespondence => {
                !automated || facts.sender_replied_to_before || facts.sender_in_contacts
            }
            SetId::Calendar => facts.has_calendar_part,
            SetId::Financial => {
                facts.has_structured_receipt
                    || category == "updates"
                    || facts.asks_for_payment_or_credentials
            }
            SetId::Security | SetId::Baseline => true,
        }
    }
}

/// The Sets relevant to this message that contain at least one needed
/// Signal, in fixed order, Baseline always last. Deterministic: the same facts
/// and the same needed names give the same answer.
pub fn select_sets(facts: &Facts, needed: &[String]) -> Vec<SetId> {
    let mut out = Vec::new();
    for set in SetId::ALL {
        if set == SetId::Baseline {
            continue;
        }
        if set.relevant(facts) && set.members().iter().any(|m| needed.iter().any(|n| n == m)) {
            out.push(set);
        }
    }
    out.push(SetId::Baseline);
    out
}

/// Assign every needed Signal to exactly one selected Set: the first that
/// lists it, else Baseline. Signals absent from every Set (user-proposed ones)
/// go to Baseline too.
pub fn assign_to_sets(sets: &[SetId], needed: &[Signal]) -> BTreeMap<SetId, Vec<Signal>> {
    let mut out: BTreeMap<SetId, Vec<Signal>> = BTreeMap::new();
    for s in needed {
        let home = sets
            .iter()
            .copied()
            .find(|set| set.members().contains(&s.name.as_str()))
            .unwrap_or(SetId::Baseline);
        out.entry(home).or_default().push(s.clone());
    }
    out
}

/// What a state builder can draw on. Everything here is already normalized;
/// the builder decides how much of it fits.
#[derive(Debug, Clone, Default)]
pub struct StateInput {
    pub subject: String,
    pub from: String,
    pub normalized_text: String,
    pub thread_state_summary: Option<String>,
    /// Named Context slices, e.g. `user_profile`, `sender_history`.
    pub context_slices: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct BuiltState {
    pub set: SetId,
    pub state: JudgeState,
    /// What was cut to fit, as "part: from→to tokens".
    pub truncated: Vec<String>,
    /// Slices actually included, in order.
    pub slices_used: Vec<String>,
    /// Hash of the Context consumed (thread summary and slices), for the
    /// cache key of Context-dependent Signals (§26).
    pub context_hash: String,
}

pub struct StateBuilder {
    pub token_budget: u32,
}

impl Default for StateBuilder {
    fn default() -> Self {
        Self {
            token_budget: 2_000,
        }
    }
}

/// Four characters per token is the estimate the whole workspace uses
/// (`JudgeState::new`); consistency matters more than precision here.
pub fn approx_tokens(s: &str) -> u32 {
    s.len().div_ceil(4) as u32
}

fn truncate_to_tokens(s: &str, tokens: u32) -> String {
    let max_chars = (tokens as usize) * 4;
    if s.len() <= max_chars {
        return s.to_string();
    }
    let end = riddle_core::floor_char_boundary(s, max_chars);
    format!("{}\n[truncated]", &s[..end])
}

impl StateBuilder {
    pub fn new(token_budget: u32) -> Self {
        Self { token_budget }
    }

    /// Build the state for one Set. Order of inclusion: the message (subject,
    /// sender, text), then the Thread State summary, then only the Context
    /// slices the Set's Signals declare. Each part is cut to what remains of
    /// the budget; the message keeps at least half the budget so a long thread
    /// summary can never crowd it out.
    pub fn build(&self, set: SetId, signals: &[Signal], input: &StateInput) -> BuiltState {
        let mut truncated = Vec::new();
        let mut remaining = self.token_budget;
        let header = format!("Subject: {}\nFrom: {}\n\n", input.subject, input.from);
        remaining = remaining.saturating_sub(approx_tokens(&header));

        let wants_thread =
            signals.iter().any(|s| s.inputs.thread) && input.thread_state_summary.is_some();
        let mut declared: Vec<String> = Vec::new();
        for s in signals {
            for slice in &s.inputs.context_slices {
                if !declared.contains(slice) && input.context_slices.contains_key(slice) {
                    declared.push(slice.clone());
                }
            }
        }
        let reserve = if wants_thread || !declared.is_empty() {
            self.token_budget / 4
        } else {
            0
        };
        let message_allow = remaining
            .saturating_sub(reserve)
            .max(self.token_budget / 2)
            .min(remaining);
        let message_tokens = approx_tokens(&input.normalized_text);
        let body = if message_tokens > message_allow {
            truncated.push(format!("message: {message_tokens}→{message_allow} tokens"));
            truncate_to_tokens(&input.normalized_text, message_allow)
        } else {
            input.normalized_text.clone()
        };
        remaining = remaining.saturating_sub(approx_tokens(&body));

        let mut text = format!("{header}{body}");
        let mut context_material = String::new();
        if wants_thread {
            if let Some(summary) = &input.thread_state_summary {
                let allow = remaining.min(self.token_budget / 8).max(50);
                let need = approx_tokens(summary);
                let part = if need > allow {
                    truncated.push(format!("thread_state: {need}→{allow} tokens"));
                    truncate_to_tokens(summary, allow)
                } else {
                    summary.clone()
                };
                remaining = remaining.saturating_sub(approx_tokens(&part));
                text.push_str("\n\nThread state: ");
                text.push_str(&part);
                context_material.push_str(&part);
            }
        }
        let mut slices_used = Vec::new();
        for name in declared {
            if remaining < 20 {
                truncated.push(format!("{name}: omitted"));
                continue;
            }
            let Some(value) = input.context_slices.get(&name) else {
                continue;
            };
            let need = approx_tokens(value);
            let part = if need > remaining {
                truncated.push(format!("{name}: {need}→{remaining} tokens"));
                truncate_to_tokens(value, remaining)
            } else {
                value.clone()
            };
            remaining = remaining.saturating_sub(approx_tokens(&part));
            text.push_str(&format!("\n\n{}: {}", name.replace('_', " "), part));
            context_material.push_str(&part);
            slices_used.push(name);
        }
        let context_hash = sha256_hex(context_material.as_bytes());
        BuiltState {
            set,
            state: JudgeState::new(text),
            truncated,
            slices_used,
            context_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::system_signal;

    fn facts_from(domain: &str) -> Facts {
        Facts {
            sender_domain: domain.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_github_sender_selects_the_github_set_and_baseline_is_always_last() {
        let needed: Vec<String> = vec!["review_requested".into(), "routine".into()];
        let sets = select_sets(&facts_from("notifications.github.com"), &needed);
        assert_eq!(sets.first(), Some(&SetId::GitHub));
        assert_eq!(sets.last(), Some(&SetId::Baseline));
        let sets = select_sets(&facts_from("example.org"), &needed);
        assert!(!sets.contains(&SetId::GitHub));
    }

    #[test]
    fn assignment_puts_each_signal_in_exactly_one_set() {
        let signals = vec![
            system_signal("review_requested").unwrap(),
            system_signal("routine").unwrap(),
            system_signal("newsletter").unwrap(),
        ];
        let sets = vec![SetId::GitHub, SetId::Baseline];
        let assigned = assign_to_sets(&sets, &signals);
        assert_eq!(assigned[&SetId::GitHub].len(), 2);
        assert_eq!(assigned[&SetId::Baseline].len(), 1);
        let total: usize = assigned.values().map(|v| v.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn the_state_builder_truncates_deterministically_and_records_the_cut() {
        let builder = StateBuilder::new(200);
        let input = StateInput {
            subject: "s".into(),
            from: "a@b".into(),
            normalized_text: "word ".repeat(1_000),
            thread_state_summary: Some("Thread of 3 messages.".into()),
            ..Default::default()
        };
        let sig = system_signal("requires_response").unwrap();
        let a = builder.build(SetId::Baseline, std::slice::from_ref(&sig), &input);
        let b = builder.build(SetId::Baseline, std::slice::from_ref(&sig), &input);
        assert_eq!(a.state.text, b.state.text);
        assert!(a.state.approx_tokens <= 230, "{}", a.state.approx_tokens);
        assert!(a.truncated.iter().any(|t| t.starts_with("message:")));
        assert!(a.state.text.contains("Thread state"));
    }

    #[test]
    fn only_declared_context_slices_enter_the_state() {
        let builder = StateBuilder::default();
        let mut slices = BTreeMap::new();
        slices.insert("user_profile".to_string(), "Role: engineer".to_string());
        slices.insert("unrelated".to_string(), "SECRET".to_string());
        let input = StateInput {
            subject: "s".into(),
            from: "a@b".into(),
            normalized_text: "hello".into(),
            thread_state_summary: None,
            context_slices: slices,
        };
        let sig = system_signal("personally_relevant").unwrap();
        let built = builder.build(SetId::Baseline, &[sig], &input);
        assert!(built.state.text.contains("Role: engineer"));
        assert!(!built.state.text.contains("SECRET"));
        assert_eq!(built.slices_used, vec!["user_profile".to_string()]);
    }
}
