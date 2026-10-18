//! The "Why?" answer (§37, §61): built from the stored trace, never
//! reconstructed. "GitHub Notifications v7 classified it based on these
//! Signals" is the current classification's Mesh plus its trace.

use riddle_core::{Explanation, MessageId, SignalLine};
use riddle_errors::{Result, RiddleError};
use riddle_store::{repo, Store};

pub fn explain(dig_store: &Store, message_id: &MessageId) -> Result<Explanation> {
    let classification = repo::classifications::current(dig_store, message_id)?
        .or(repo::classifications::current_shadow(
            dig_store, message_id,
        )?)
        .ok_or_else(|| RiddleError::not_found("the message has not been sifted yet"))?;
    let trace = repo::traces::get(dig_store, &classification.trace_id)?;
    let signals = trace
        .as_ref()
        .map(|t| {
            t.signals
                .iter()
                .map(|e| SignalLine {
                    name: e.signal_name.clone(),
                    version: e.signal_version,
                    score: e.score(None),
                    selected: e.selected.clone(),
                    cached: e.cached,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Explanation {
        message_id: message_id.clone(),
        disposition: classification.disposition.clone(),
        mechanism: classification.mechanism,
        mesh_name: trace.as_ref().and_then(|t| t.mesh_name.clone()),
        mesh_version: classification.mesh_version,
        matched: classification.matched_rule.clone(),
        signals,
        gate: trace.as_ref().and_then(|t| t.gate.clone()),
        escalated: trace.as_ref().is_some_and(|t| t.escalation.is_some()),
        llm_rationale: trace
            .as_ref()
            .and_then(|t| t.escalation.as_ref())
            .map(|e| e.rationale.clone()),
        trace_id: classification.trace_id.clone(),
        shadow: classification.shadow,
        reason: classification.reason,
        // Replaying the Mesh needs the index store; the app's explain does it.
        routes: vec![],
        importance_set_by_you: repo::importance::get(dig_store, message_id)?.is_some(),
        verification_code: repo::facts::get(dig_store, message_id)?
            .map(|f| f.verification_code)
            .filter(|c| !c.is_empty()),
        importance: repo::importance::get(dig_store, message_id)?
            .unwrap_or(classification.importance),
        earlier_meshes: trace
            .as_ref()
            .map(|t| t.earlier_meshes())
            .unwrap_or_default(),
    })
}
