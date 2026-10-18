//! A decision previewed, not made (17 Oct 2026). After a Mesh or a list
//! changed, the only ways to learn what a message would be decided now
//! were to send it back to Sift (which rewrote its classification) or to
//! simulate the whole Mesh over history. `decide_preview` runs the
//! pipeline through `Sift::run`, the non-persisting form, under today's
//! Meshes, lists and Signals, and returns the would-be classification and
//! trace beside the stored ones with the difference in one sentence. The
//! Signal cache is read as in a pass and a fresh judge call is booked as
//! usage like any other; the classification, its trace and the Find stay
//! as they were.

use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_sift::Sift;
use riddle_store::repo;

impl App {
    /// What today's rules would decide for one message, beside what stands.
    /// Nothing is written.
    pub async fn decide_preview(&self, dig_id: &DigId, id: &MessageId) -> Result<DecisionPreview> {
        let dig = self.dig(dig_id)?;
        let store = self.stores.dig(dig_id)?;
        let message = repo::messages::get(&store, dig_id, id)?
            .ok_or_else(|| RiddleError::not_found(format!("message {id}")))?;
        let stored = repo::classifications::current(&store, id)?;
        let stored_trace = match &stored {
            Some(k) => repo::traces::get(&store, &k.trace_id)?,
            None => None,
        };
        // The pass's budget cap is for unattended spending; a preview is one
        // call the user asked for, so it runs with the services' own judge
        // and refiner and no pass cap.
        let sift = Sift::from_services(&self.services);
        let mut input = self.sift_input(&dig, message)?;
        input.pass_id = format!("preview-{}", riddle_sift::new_pass_id());
        let outcome = crate::ai::CURRENT_DIG
            .scope(dig_id.clone(), sift.run(input))
            .await?;
        let (changed, diff) = preview_diff(
            stored.as_ref(),
            stored_trace.as_ref(),
            &outcome.classification,
            &outcome.trace,
        );
        Ok(DecisionPreview {
            message_id: id.clone(),
            dig_id: dig_id.clone(),
            stored,
            stored_trace,
            would: outcome.classification,
            would_trace: outcome.trace,
            changed,
            diff,
            note: outcome.note,
        })
    }
}

/// "Find: Review", "Spoil".
fn disposition_words(d: &Disposition) -> String {
    match d {
        Disposition::Find { find_type } => format!("Find: {}", find_type.label()),
        Disposition::Filed => "Filed".into(),
        Disposition::Spoil => "Spoil".into(),
        Disposition::Contamination => "Contamination".into(),
        Disposition::Unresolved => "Unresolved".into(),
    }
}

/// Who decided, in words: the Mesh version, or the mechanism when no Mesh
/// took part (the same words the Sift Activity uses).
fn decider_words(k: &Classification, trace: Option<&DecisionTrace>) -> String {
    if let Some(name) = trace.and_then(|t| t.mesh_name.as_deref()) {
        return format!("{name} v{}", k.mesh_version.unwrap_or(0));
    }
    match k.mechanism {
        DecisionMechanism::Gate => "a gate".into(),
        DecisionMechanism::User => "you".into(),
        DecisionMechanism::Director => "the Director".into(),
        DecisionMechanism::Provider => "the mailbox".into(),
        DecisionMechanism::Temporal => "time".into(),
        DecisionMechanism::Llm => "the LLM".into(),
        DecisionMechanism::Jev => "the judge".into(),
        DecisionMechanism::Deterministic | DecisionMechanism::Mesh => "deterministic facts".into(),
    }
}

fn strata_words(s: &[String]) -> String {
    if s.is_empty() {
        "none".into()
    } else {
        s.join(", ")
    }
}

/// Whether the would-be decision differs from the stored one (disposition,
/// deciding Mesh or Strata), and the difference in one sentence: "would be
/// Filed by Bills v4 instead of Spoil by Frequent promotions v2".
pub fn preview_diff(
    stored: Option<&Classification>,
    stored_trace: Option<&DecisionTrace>,
    would: &Classification,
    would_trace: &DecisionTrace,
) -> (bool, String) {
    let w = disposition_words(&would.disposition);
    let wb = decider_words(would, Some(would_trace));
    let Some(s) = stored else {
        return (true, format!("would be {w} by {wb}; not decided before"));
    };
    let sw = disposition_words(&s.disposition);
    let sb = decider_words(s, stored_trace);
    let same_mesh = s.mesh_id == would.mesh_id && s.mesh_version == would.mesh_version;
    let same_strata = s.strata == would.strata;
    if s.disposition != would.disposition {
        return (
            true,
            format!("would be {w} by {wb} instead of {sw} by {sb}"),
        );
    }
    if !same_mesh {
        return (true, format!("still {w}, but by {wb} instead of {sb}"));
    }
    if !same_strata {
        return (
            true,
            format!(
                "still {w} by {wb}, with Strata {} instead of {}",
                strata_words(&would.strata),
                strata_words(&s.strata)
            ),
        );
    }
    (false, format!("the same: {w} by {wb}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classification(
        d: Disposition,
        mesh: Option<(&str, u32)>,
        strata: &[&str],
    ) -> Classification {
        Classification {
            id: ClassificationId::new(),
            message_id: MessageId::from("m1"),
            dig_id: DigId::from("d1"),
            disposition: d,
            strata: strata.iter().map(|s| s.to_string()).collect(),
            notify: false,
            mechanism: if mesh.is_some() {
                DecisionMechanism::Mesh
            } else {
                DecisionMechanism::Gate
            },
            mesh_id: mesh.map(|(id, _)| MeshId::from(id)),
            mesh_version: mesh.map(|(_, v)| v),
            matched_rule: None,
            confidence: None,
            reason: String::new(),
            trace_id: TraceId::new(),
            shadow: false,
            created_at: chrono::Utc::now(),
            superseded_at: None,
            series: None,
            importance: Importance::Normal,
        }
    }

    fn trace(k: &Classification, name: Option<&str>) -> DecisionTrace {
        DecisionTrace {
            id: k.trace_id.clone(),
            message_id: k.message_id.clone(),
            dig_id: k.dig_id.clone(),
            objective: String::new(),
            strategy: String::new(),
            facts: Facts::default(),
            gate: None,
            signals: vec![],
            mesh_id: k.mesh_id.clone(),
            mesh_version: k.mesh_version,
            mesh_name: name.map(str::to_string),
            matched_rule: None,
            guard_vetoes: vec![],
            escalation: None,
            conclusion: k.disposition.clone(),
            confidence: None,
            mechanism: k.mechanism,
            timing: TraceTiming::default(),
            cost: TraceCost::default(),
            judge_identity: None,
            app_version: String::new(),
            build_stamp: String::new(),
            shadow: false,
            created_at: k.created_at,
            pass_id: String::new(),
            list_results: vec![],
            policy: None,
            margin: None,
            mesh_outcomes: vec![],
        }
    }

    #[test]
    fn a_different_disposition_names_both_deciders() {
        let stored = classification(Disposition::Spoil, Some(("promo", 2)), &[]);
        let would = classification(Disposition::Filed, Some(("bills", 4)), &["Finance"]);
        let (changed, diff) = preview_diff(
            Some(&stored),
            Some(&trace(&stored, Some("Frequent promotions"))),
            &would,
            &trace(&would, Some("Bills")),
        );
        assert!(changed);
        assert_eq!(
            diff,
            "would be Filed by Bills v4 instead of Spoil by Frequent promotions v2"
        );
    }

    #[test]
    fn the_same_decision_by_the_same_mesh_is_unchanged() {
        let stored = classification(Disposition::Spoil, Some(("promo", 2)), &["Shopping"]);
        let would = classification(Disposition::Spoil, Some(("promo", 2)), &["Shopping"]);
        let (changed, diff) = preview_diff(
            Some(&stored),
            Some(&trace(&stored, Some("Frequent promotions"))),
            &would,
            &trace(&would, Some("Frequent promotions")),
        );
        assert!(!changed);
        assert_eq!(diff, "the same: Spoil by Frequent promotions v2");
    }

    #[test]
    fn a_new_mesh_version_taking_the_same_decision_is_still_a_change() {
        let stored = classification(Disposition::Spoil, Some(("promo", 2)), &[]);
        let would = classification(Disposition::Spoil, Some(("promo", 3)), &[]);
        let (changed, diff) = preview_diff(
            Some(&stored),
            Some(&trace(&stored, Some("Frequent promotions"))),
            &would,
            &trace(&would, Some("Frequent promotions")),
        );
        assert!(changed);
        assert_eq!(
            diff,
            "still Spoil, but by Frequent promotions v3 instead of Frequent promotions v2"
        );
    }

    #[test]
    fn a_message_never_decided_says_so_and_a_gate_is_named_as_one() {
        let would = classification(
            Disposition::Find {
                find_type: FindType::Review,
            },
            None,
            &[],
        );
        let (changed, diff) = preview_diff(None, None, &would, &trace(&would, None));
        assert!(changed);
        assert_eq!(diff, "would be Find: Review by a gate; not decided before");
    }
}
