//! Mesh simulation (§38): replay a candidate over the messages of the window
//! using stored facts and cached Signal evaluations, without calling the
//! judge. Messages whose Signals are not cached are counted as needing a
//! judge call and left in place (they cannot be said to change), and the
//! estimated cost uses the 2,000-token state the budget allows.
//!
//! List membership comes from the stored facts alone, so the simulator never
//! loads message bodies: `sender_in_never_hidden`, `sender_in_contacts`,
//! `sender_is_vip` and `same_organisation` were computed at Sift time from
//! the same profile. Family is not a fact, so a family guard is simulated as
//! not matching; the report says so in `notes`.
//!
//! Every message in the window is read. It used to read the newest 5,000
//! per Dig and drop the rest without a word, so on a mailbox with more than
//! that in "100 days", applying a Mesh to history left the oldest weeks
//! undecided while its preview said the window was covered. Past a hard
//! ceiling it now refuses, naming the Dig and the count.

use crate::eval::{evaluate_with, ListMembership};
pub use crate::policy::ConfidencePolicy;
use riddle_core::{
    DecisionMechanism, Dig, Disposition, DispositionKind, Facts, Mesh, MeshStatus, NamedList,
    SimulationReport, Transition,
};
use riddle_errors::{Result, RiddleError};
use riddle_store::{repo, Stores};
use std::collections::HashMap;

/// List membership from stored facts, and the user's named lists.
pub struct FactsLists<'a>(pub &'a Facts, pub &'a riddle_core::NamedLists);

impl ListMembership for FactsLists<'_> {
    fn is_member(&self, list: NamedList) -> bool {
        match list {
            NamedList::NeverHidden => self.0.sender_in_never_hidden,
            NamedList::Contacts => self.0.sender_in_contacts || self.0.sender_replied_to_before,
            NamedList::Vips => self.0.sender_is_vip,
            NamedList::Organisation => self.0.same_organisation,
            NamedList::Family => false,
        }
    }
    fn in_named_list(&self, list: &str) -> bool {
        self.1.contains(list, &self.0.sender)
    }
}

pub struct SimulationInput<'a> {
    pub stores: &'a Stores,
    pub digs: Vec<Dig>,
    pub candidate: &'a Mesh,
    pub window_days: u32,
    pub now: chrono::DateTime<chrono::Utc>,
    /// The most messages one Dig's window may hold; more is an error, not
    /// a quiet cut.
    pub max_messages_per_dig: u32,
    pub price_per_mtok: Option<f64>,
    /// The confidence policy Sift applies after routing; the simulation
    /// applies the same one, so a route that would be held for escalation
    /// is predicted as Unresolved, not as what it routed to.
    pub policy: ConfidencePolicy,
}

impl<'a> SimulationInput<'a> {
    pub fn new(stores: &'a Stores, digs: Vec<Dig>, candidate: &'a Mesh) -> Self {
        Self {
            stores,
            digs,
            candidate,
            window_days: 90,
            now: chrono::Utc::now(),
            max_messages_per_dig: 50_000,
            price_per_mtok: Some(0.042),
            policy: ConfidencePolicy::default(),
        }
    }
}

fn kind_of(k: Option<&riddle_core::Classification>) -> DispositionKind {
    k.map(|c| c.disposition.kind())
        .unwrap_or(DispositionKind::Unresolved)
}

/// One message whose disposition the candidate would change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedMessage {
    pub dig_id: riddle_core::DigId,
    pub message_id: riddle_core::MessageId,
    /// The current decision is the user's own, which a rule change never
    /// re-decides. Read here, where the classification is already loaded,
    /// so "apply to history" need not read it again per message.
    pub user_decided: bool,
}

pub fn simulate(input: SimulationInput<'_>) -> Result<SimulationReport> {
    Ok(simulate_detailed(input)?.0)
}

/// `simulate`, plus every changed message rather than a sample: what
/// `apply_mesh_to_history` re-decides.
pub fn simulate_detailed(
    input: SimulationInput<'_>,
) -> Result<(SimulationReport, Vec<ChangedMessage>)> {
    let index = input.stores.index();
    let named = repo::lists::named(&index)?;
    let candidate = input.candidate;
    let from_version = repo::meshes::latest(&index, &candidate.id)?
        .map(|m| m.version)
        .filter(|v| *v != candidate.version || candidate.status != MeshStatus::Draft);
    let mut running: Vec<Mesh> = repo::meshes::list_running(&index)?
        .into_iter()
        .filter(|m| m.id != candidate.id)
        .collect();
    let mut cand = candidate.clone();
    cand.status = MeshStatus::Active;
    running.push(cand);

    let since = input.now - chrono::Duration::days(input.window_days as i64);
    let mut historical = 0u64;
    let mut unchanged = 0u64;
    let mut changed = 0u64;
    let mut needs_judge = 0u64;
    let mut joins_series = 0u64;
    let mut recorded = 0u64;
    let mut taken_first_by: HashMap<riddle_core::MeshId, u64> = HashMap::new();
    let mut takes_from: HashMap<riddle_core::MeshId, u64> = HashMap::new();
    let mut rescore = 0u64;
    // `name@vN` for every Signal the candidate reads, in the form missing
    // evaluations are reported in.
    let candidate_refs: Vec<String> = {
        let mut refs = Vec::new();
        for r in &candidate.routes {
            r.condition.signal_refs(&mut refs);
        }
        for g in &candidate.guards {
            g.condition.signal_refs(&mut refs);
        }
        refs.into_iter().map(|(s, v)| format!("{s}@v{v}")).collect()
    };
    let mut transitions: HashMap<(DispositionKind, DispositionKind), u64> = HashMap::new();
    let mut sample_changed = Vec::new();
    let mut all_changed = Vec::new();
    for dig in input
        .digs
        .iter()
        .filter(|d| candidate.scope.applies_to(&d.id, d.group.as_deref()))
    {
        let store = input.stores.dig(&dig.id)?;
        let in_scope: Vec<Mesh> = running
            .iter()
            .filter(|m| m.scope.applies_to(&dig.id, dig.group.as_deref()))
            .cloned()
            .collect();
        let (alone, others): (Vec<Mesh>, Vec<Mesh>) =
            in_scope.iter().cloned().partition(|m| m.id == candidate.id);
        // One past the ceiling tells an over-full window from a full one.
        let ids = repo::messages::ids_since(&store, &since, input.max_messages_per_dig + 1)?;
        if ids.len() > input.max_messages_per_dig as usize {
            return Err(RiddleError::invalid(format!(
                "Dig {} holds more than {} messages in the last {} days, the most a simulation reads. Use a shorter window.",
                dig.name, input.max_messages_per_dig, input.window_days
            )));
        }
        for id in ids {
            historical += 1;
            let Some(facts) = repo::facts::get(&store, &id)? else {
                unchanged += 1;
                continue;
            };
            let classification = repo::classifications::current(&store, &id)?;
            // Sent mail, drafts and gate decisions are not a Mesh's to make;
            // re-sifting them decides them the same way, so they would only
            // pad the preview with changes that never happen.
            if facts.is_sent
                || facts.is_draft
                || classification
                    .as_ref()
                    .is_some_and(|k| k.mechanism == DecisionMechanism::Gate)
            {
                unchanged += 1;
                continue;
            }
            let evaluations = repo::signals::evaluations_for(&store, &id)?;
            let current = kind_of(classification.as_ref());
            let decision = evaluate_with(
                &in_scope,
                &facts,
                &evaluations,
                &FactsLists(&facts, &named),
                Some(&input.policy),
            );
            if !decision.unresolved_signal_refs.is_empty() {
                needs_judge += 1;
                unchanged += 1;
                // The rule being applied reads a Signal this message was
                // never scored on (a new Signal, or a new version of one):
                // apply sends it back so Sift scores it, rather than leaving
                // history decided without the Signal the rule now needs.
                if decision
                    .unresolved_signal_refs
                    .iter()
                    .any(|r| candidate_refs.contains(r))
                {
                    rescore += 1;
                    all_changed.push(ChangedMessage {
                        dig_id: dig.id.clone(),
                        message_id: id.clone(),
                        user_decided: classification
                            .as_ref()
                            .is_some_and(|k| k.mechanism == DecisionMechanism::User),
                    });
                }
                continue;
            }
            // Who else claims this message: the Mesh that decides it before
            // the candidate gets a turn, or the one the candidate takes it from.
            let winner = decision
                .output
                .as_ref()
                .and_then(|o| o.disposition.as_ref())
                .filter(|d| {
                    !decision.vetoed()
                        && input
                            .policy
                            .stands(d.kind(), decision.margin, &facts.language)
                })
                .and(decision.mesh_id.clone());
            match &winner {
                Some(w) if w == &candidate.id => {
                    if let Some(y) = decider(&others, &facts, &evaluations, &named, &input.policy) {
                        *takes_from.entry(y.id.clone()).or_insert(0) += 1;
                    }
                }
                Some(w)
                    if decider(&alone, &facts, &evaluations, &named, &input.policy).is_some() =>
                {
                    *taken_first_by.entry(w.clone()).or_insert(0) += 1;
                }
                _ => {}
            }
            let routed = match decision.output.as_ref() {
                Some(o) if o.escalate && o.disposition.is_none() => DispositionKind::Unresolved,
                Some(o) => o
                    .disposition
                    .as_ref()
                    .map(Disposition::kind)
                    .unwrap_or(DispositionKind::Unresolved),
                None => DispositionKind::Unresolved,
            };
            let to = if input
                .policy
                .stands(routed, decision.margin, &facts.language)
            {
                routed
            } else {
                DispositionKind::Unresolved
            };
            // Same disposition, but recorded differently: it must be decided
            // again, or the store keeps the old Find type, Strata or series.
            let effect = if to == current && to != DispositionKind::Unresolved {
                classification
                    .as_ref()
                    .zip(decision.output.as_ref())
                    .zip(decision.mesh_id.as_ref())
                    .and_then(|((k, o), mesh)| {
                        effect_change(k, o, mesh, &facts.sender, || {
                            repo::finds::for_message(&index, &id)
                                .ok()
                                .and_then(|fs| fs.into_iter().find_map(|f| f.series))
                                .map(|s| s.key)
                        })
                    })
            } else {
                None
            };
            if let Some(effect) = effect {
                unchanged += 1;
                match effect {
                    EffectChange::JoinsSeries => joins_series += 1,
                    EffectChange::Recorded => recorded += 1,
                }
            } else if to == current {
                unchanged += 1;
                continue;
            } else {
                changed += 1;
                *transitions.entry((current, to)).or_insert(0) += 1;
                if sample_changed.len() < 20 {
                    sample_changed.push(id.clone());
                }
            }
            all_changed.push(ChangedMessage {
                dig_id: dig.id.clone(),
                message_id: id.clone(),
                user_decided: classification
                    .as_ref()
                    .is_some_and(|k| k.mechanism == DecisionMechanism::User),
            });
        }
    }
    let mut transitions: Vec<Transition> = transitions
        .into_iter()
        .map(|((from, to), count)| Transition { from, to, count })
        .collect();
    transitions.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.from.as_str().cmp(b.from.as_str()))
            .then(a.to.as_str().cmp(b.to.as_str()))
    });
    Ok((
        SimulationReport {
            mesh_id: Some(candidate.id.clone()),
            mesh_name: candidate.name.clone(),
            from_version,
            to_version: candidate.version,
            historical_messages: historical,
            unchanged,
            changed,
            transitions,
            new_judge_calls_required: needs_judge,
            estimated_cost_usd: input
                .price_per_mtok
                .map(|p| needs_judge as f64 * 2_000.0 * p / 1_000_000.0),
            sample_changed,
            window_days: input.window_days,
            joins_series,
            recorded_changes: recorded,
            rescore,
            taken_first_by: overlaps(taken_first_by, &running),
            takes_from: overlaps(takes_from, &running),
        },
        all_changed,
    ))
}

/// The Mesh that decides a message under `meshes`: a route matched, stood
/// under the confidence policy, and no guard vetoed it. `None` when nothing
/// decides. The preview's overlaps and the Order view both ask this, so
/// "taken first by" means the same thing in each.
pub(crate) fn decider<'m>(
    meshes: &'m [Mesh],
    facts: &Facts,
    evaluations: &[riddle_core::SignalEvaluation],
    named: &riddle_core::NamedLists,
    policy: &ConfidencePolicy,
) -> Option<&'m Mesh> {
    let d = evaluate_with(
        meshes,
        facts,
        evaluations,
        &FactsLists(facts, named),
        Some(policy),
    );
    if d.vetoed() {
        return None;
    }
    let kind = d.output.as_ref()?.disposition.as_ref()?.kind();
    if !policy.stands(kind, d.margin, &facts.language) {
        return None;
    }
    meshes.iter().find(|m| Some(&m.id) == d.mesh_id.as_ref())
}

/// Counts by Mesh, named and most first.
pub(crate) fn overlaps(
    counts: HashMap<riddle_core::MeshId, u64>,
    meshes: &[Mesh],
) -> Vec<riddle_core::MeshOverlap> {
    let mut out: Vec<riddle_core::MeshOverlap> = counts
        .into_iter()
        .filter_map(|(id, count)| {
            meshes
                .iter()
                .find(|m| m.id == id)
                .map(|m| riddle_core::MeshOverlap {
                    mesh_id: id,
                    mesh_name: m.name.clone(),
                    priority: m.priority,
                    count,
                })
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.mesh_name.cmp(&b.mesh_name)));
    out
}

/// How a decision that keeps its disposition would still be recorded
/// differently under the candidate.
enum EffectChange {
    /// A `+ Replaces` route now claims it, or claims it for another series.
    JoinsSeries,
    /// Another Find type, other Strata, Notify, or no longer in a series.
    Recorded,
}

/// The one comparison of "what would be recorded" behind apply to history.
/// It compared the disposition alone, so a Mesh version that turned Review
/// Finds into Respond Finds, relabelled a Stratum or dropped `+ Replaces`
/// previewed as "no change" and left 104 GitHub messages on the old rule's
/// decision. The Find's series lives on the Find, so it is read only when
/// needed.
fn effect_change(
    current: &riddle_core::Classification,
    output: &riddle_core::MeshOutput,
    mesh: &riddle_core::MeshId,
    sender: &str,
    find_series: impl FnOnce() -> Option<String>,
) -> Option<EffectChange> {
    let is_find = matches!(current.disposition, Disposition::Find { .. });
    let series_now = output
        .replaces
        .then(|| riddle_core::series_key(mesh, sender));
    let series_before = if is_find {
        find_series()
    } else {
        current.series.clone()
    };
    if series_now != series_before {
        return Some(if series_now.is_some() {
            EffectChange::JoinsSeries
        } else {
            EffectChange::Recorded
        });
    }
    let find_type_differs = matches!(
        (&current.disposition, &output.disposition),
        (Disposition::Find { find_type: a }, Some(Disposition::Find { find_type: b })) if a != b
    );
    let mut strata_before = current.strata.clone();
    let mut strata_now = output.strata.clone();
    strata_before.sort();
    strata_now.sort();
    // A route's own level is part of what it decides; without one the level
    // is derived from the decision, which the other checks already cover.
    let importance_differs = output.importance.is_some_and(|i| i != current.importance);
    (find_type_differs
        || strata_before != strata_now
        || current.notify != output.notify
        || importance_differs)
        .then_some(EffectChange::Recorded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use riddle_core::*;

    fn message(dig: &DigId, pid: &str, from: &str) -> Message {
        Message {
            id: MessageId::for_provider(dig, pid),
            dig_id: dig.clone(),
            thread_id: ThreadId::for_provider(dig, pid),
            provider_id: pid.into(),
            subject: "s".into(),
            from: Address::new(from, None),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            date: chrono::Utc::now(),
            snippet: String::new(),
            labels: vec![],
            headers: MessageHeaders::default(),
            attachments: vec![],
            size_bytes: 0,
            is_sent: false,
            has_body: false,
            body_text: None,
            body_html: None,
            synced_at: chrono::Utc::now(),
        }
    }

    fn dig(name: &str, group: Option<&str>) -> Dig {
        Dig {
            id: DigId("d1".into()),
            name: name.into(),
            email: "me@x".into(),
            provider: ProviderKind::Fixture,
            group: group.map(Into::into),
            privacy: PrivacyMode::CloudJudgment,
            projection: ProjectionMode::None,
            status: DigStatus::Synced,
            shadow_started_at: None,
            created_at: chrono::Utc::now(),
            disabled: false,
            mailbox: None,
        }
    }

    fn eval(id: &MessageId, name: &str, yes: f64) -> SignalEvaluation {
        let mut d = Distribution::new();
        d.insert("YES".into(), yes);
        d.insert("NO".into(), 1.0 - yes);
        SignalEvaluation {
            message_id: id.clone(),
            signal_name: name.into(),
            signal_version: 1,
            engine_identity: "t".into(),
            selected: "YES".into(),
            distribution: d.clone(),
            calibrated: d,
            confidence: yes,
            latency_ms: 0,
            input_tokens: 0,
            cached: true,
            deterministic: true,
            context_hash: None,
            request_hash: None,
            evaluated_at: chrono::Utc::now(),
        }
    }

    fn classify(store: &riddle_store::Store, id: &MessageId, dig: &DigId, d: Disposition) {
        repo::classifications::record(
            store,
            &Classification {
                id: ClassificationId::new(),
                message_id: id.clone(),
                dig_id: dig.clone(),
                importance: Importance::default_for(&d),
                disposition: d,
                strata: vec![],
                notify: false,
                mechanism: DecisionMechanism::Mesh,
                mesh_id: None,
                mesh_version: None,
                matched_rule: None,
                confidence: None,
                reason: String::new(),
                trace_id: TraceId::new(),
                shadow: false,
                created_at: chrono::Utc::now(),
                superseded_at: None,
                series: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_margin_that_rounds_to_the_requirement_says_it_is_just_under() {
        let policy = ConfidencePolicy::default();
        let near = policy
            .hold_reason(DispositionKind::Spoil, Some(0.1494), "en")
            .unwrap();
        assert!(
            near.contains("0.149 is just under the 0.15 required"),
            "{near}"
        );
        let far = policy
            .hold_reason(DispositionKind::Spoil, Some(0.08), "en")
            .unwrap();
        assert!(far.contains("0.08 is below the 0.15 required"), "{far}");
    }

    #[test]
    fn a_simulation_holds_a_route_whose_signal_margin_sift_would_not_stand_by() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("d", None);
        let store = stores.dig(&dig.id).unwrap();
        // m1 clears .8 by .17: Spoil stands. m2 clears it by .05: Sift would
        // hold it for escalation, so the simulation says Unresolved too.
        // m3 is French: held whatever the margin.
        for (pid, score, language) in [("m1", 0.97, "en"), ("m2", 0.85, "en"), ("m3", 0.97, "fr")] {
            let m = message(&dig.id, pid, "news@list.example");
            repo::messages::upsert(&store, &m).unwrap();
            repo::facts::put(
                &store,
                &m.id,
                &Facts {
                    mailing_list: true,
                    language: language.into(),
                    ..Default::default()
                },
            )
            .unwrap();
            repo::signals::put_evaluation(&store, &eval(&m.id, "newsletter", score)).unwrap();
            classify(&store, &m.id, &dig.id, Disposition::Filed);
        }
        let mut candidate =
            parse("Mesh: Newsletters v2\nScope:\nAll Digs\nROUTES\nIF newsletter@v1 > .8 → Spoil")
                .unwrap();
        candidate.id = MeshId("mesh_news".into());
        let report =
            simulate(SimulationInput::new(&stores, vec![dig.clone()], &candidate)).unwrap();
        let count = |from: DispositionKind, to: DispositionKind| {
            report
                .transitions
                .iter()
                .find(|t| t.from == from && t.to == to)
                .map(|t| t.count)
                .unwrap_or(0)
        };
        assert_eq!(count(DispositionKind::Filed, DispositionKind::Spoil), 1);
        assert_eq!(
            count(DispositionKind::Filed, DispositionKind::Unresolved),
            2
        );
        // A fact-only route has no margin and always stands.
        let mut facts_only =
            parse("Mesh: Lists v1\nScope:\nAll Digs\nROUTES\nIF mailing_list → Spoil").unwrap();
        facts_only.id = MeshId("mesh_lists".into());
        let report = simulate(SimulationInput::new(&stores, vec![dig], &facts_only)).unwrap();
        assert_eq!(report.changed, 3);
    }

    #[test]
    fn a_simulation_reads_the_whole_window_and_refuses_one_past_its_ceiling() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("Gmail", None);
        let store = stores.dig(&dig.id).unwrap();
        for n in 0..5 {
            let m = message(&dig.id, &format!("m{n}"), "news@list.example");
            repo::messages::upsert(&store, &m).unwrap();
            repo::facts::put(
                &store,
                &m.id,
                &Facts {
                    mailing_list: true,
                    ..Default::default()
                },
            )
            .unwrap();
            classify(&store, &m.id, &dig.id, Disposition::Unresolved);
        }
        let mut candidate = parse("Mesh: Lists v1\nROUTES\nIF mailing_list → Spoil").unwrap();
        candidate.id = MeshId("mesh_lists".into());

        let mut input = SimulationInput::new(&stores, vec![dig.clone()], &candidate);
        input.max_messages_per_dig = 5;
        let report = simulate(input).unwrap();
        assert_eq!(
            report.historical_messages, 5,
            "every message, not the newest few"
        );
        assert_eq!(report.changed, 5);

        let mut input = SimulationInput::new(&stores, vec![dig], &candidate);
        input.max_messages_per_dig = 4;
        let e = simulate(input).unwrap_err();
        assert!(
            e.message.contains("holds more than 4 messages"),
            "{}",
            e.message
        );
    }

    #[test]
    fn mail_never_scored_on_a_signal_the_rule_reads_is_sent_back_to_be_scored() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("Gmail", None);
        let store = stores.dig(&dig.id).unwrap();
        for pid in ["m1", "m2"] {
            let m = message(&dig.id, pid, "news@list.example");
            repo::messages::upsert(&store, &m).unwrap();
            repo::facts::put(
                &store,
                &m.id,
                &Facts {
                    mailing_list: true,
                    ..Default::default()
                },
            )
            .unwrap();
            classify(&store, &m.id, &dig.id, Disposition::Unresolved);
        }
        // m1 was scored on marketing v1 long ago, never on v2.
        let m1 = MessageId::for_provider(&dig.id, "m1");
        repo::signals::put_evaluation(&store, &eval(&m1, "marketing", 0.9)).unwrap();

        let mut candidate = parse("Mesh: Promos v2\nROUTES\nIF marketing@v2 > .8 → Spoil").unwrap();
        candidate.id = MeshId("mesh_promos".into());
        let (report, changed) =
            simulate_detailed(SimulationInput::new(&stores, vec![dig.clone()], &candidate))
                .unwrap();
        assert_eq!(report.rescore, 2, "neither was scored on v2");
        assert_eq!(changed.len(), 2);

        // A Signal only another Mesh reads is not this rule's to re-score.
        let mut other = parse("Mesh: Lists v1\nROUTES\nIF mailing_list → Filed").unwrap();
        other.id = MeshId("mesh_lists".into());
        other.status = MeshStatus::Active;
        let mut sig_mesh =
            parse("Mesh: Asks v1\nPriority: -1\nROUTES\nIF requires_action@v1 > .8 → Find: Do")
                .unwrap();
        sig_mesh.id = MeshId("mesh_asks".into());
        sig_mesh.status = MeshStatus::Active;
        repo::meshes::upsert(&stores.index(), &sig_mesh).unwrap();
        let (report, _) =
            simulate_detailed(SimulationInput::new(&stores, vec![dig], &other)).unwrap();
        assert_eq!(report.rescore, 0);
    }

    #[test]
    fn a_new_find_type_or_stratum_is_re_decided_though_the_disposition_stays() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("Gmail", None);
        let store = stores.dig(&dig.id).unwrap();
        for (pid, facts, current) in [
            (
                "review",
                Facts {
                    mailing_list: true,
                    ..Default::default()
                },
                Disposition::Find {
                    find_type: FindType::Review,
                },
            ),
            ("filed", Facts::default(), Disposition::Filed),
            (
                "same",
                Facts {
                    mailing_list: true,
                    ..Default::default()
                },
                Disposition::Find {
                    find_type: FindType::Respond,
                },
            ),
        ] {
            let m = message(&dig.id, pid, "notifications@example.com");
            repo::messages::upsert(&store, &m).unwrap();
            repo::facts::put(&store, &m.id, &facts).unwrap();
            classify(&store, &m.id, &dig.id, current);
        }
        let candidate = parse(
            "Mesh: Threads\nROUTES\nIF mailing_list → Find: Respond\nELSE → Filed + Stratum: Engineering",
        )
        .unwrap();
        let (report, changed) =
            simulate_detailed(SimulationInput::new(&stores, vec![dig], &candidate)).unwrap();
        assert_eq!(report.changed, 0, "no disposition changes");
        assert_eq!(
            report.recorded_changes, 2,
            "a Review Find becomes Respond; Filed gains a Stratum"
        );
        let ids: Vec<&str> = changed.iter().map(|c| c.message_id.provider_id()).collect();
        assert_eq!(ids.len(), 2);
        assert!(!ids.contains(&"same"), "already recorded as the rule would");
    }

    #[test]
    fn a_preview_names_the_mesh_that_takes_its_mail_first_and_the_one_it_takes_from() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("Gmail", None);
        let store = stores.dig(&dig.id).unwrap();
        for pid in ["a", "b"] {
            let m = message(&dig.id, pid, "x@github.com");
            repo::messages::upsert(&store, &m).unwrap();
            let facts = Facts {
                sender_domain: "github.com".into(),
                mailing_list: true,
                ..Default::default()
            };
            repo::facts::put(&store, &m.id, &facts).unwrap();
            classify(&store, &m.id, &dig.id, Disposition::Filed);
        }
        let mut lists = parse("Mesh: Lists\nPriority: 0\nROUTES\nIF mailing_list → Filed").unwrap();
        lists.id = MeshId("lists".into());
        lists.status = MeshStatus::Active;
        repo::meshes::upsert(&stores.index(), &lists).unwrap();

        let mut late =
            parse("Mesh: GitHub\nPriority: 10\nROUTES\nIF sender_domain = github.com → Spoil")
                .unwrap();
        late.id = MeshId("gh".into());
        let report = simulate(SimulationInput::new(&stores, vec![dig.clone()], &late)).unwrap();
        assert_eq!(report.changed, 0);
        assert_eq!(report.taken_first_by.len(), 1);
        assert_eq!(
            (
                report.taken_first_by[0].mesh_name.as_str(),
                report.taken_first_by[0].count
            ),
            ("Lists", 2)
        );

        late.priority = -10;
        let report = simulate(SimulationInput::new(&stores, vec![dig], &late)).unwrap();
        assert_eq!(report.changed, 2);
        assert!(report.taken_first_by.is_empty());
        assert_eq!(
            (
                report.takes_from[0].mesh_name.as_str(),
                report.takes_from[0].count
            ),
            ("Lists", 2)
        );
    }

    #[test]
    fn sent_mail_and_drafts_are_never_counted_as_a_rule_change() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("Gmail", None);
        let store = stores.dig(&dig.id).unwrap();
        for (pid, facts) in [
            (
                "sent",
                Facts {
                    is_sent: true,
                    ..Default::default()
                },
            ),
            (
                "draft",
                Facts {
                    is_draft: true,
                    ..Default::default()
                },
            ),
            ("inbox", Facts::default()),
        ] {
            let m = message(&dig.id, pid, "me@example.com");
            repo::messages::upsert(&store, &m).unwrap();
            repo::facts::put(&store, &m.id, &facts).unwrap();
            classify(&store, &m.id, &dig.id, Disposition::Filed);
        }
        let candidate = parse("Mesh: All\nROUTES\nIF thread_length >= 0 → Spoil").unwrap();
        let report = simulate(SimulationInput::new(&stores, vec![dig], &candidate)).unwrap();
        assert_eq!(
            report.changed, 1,
            "only the received message is a Mesh's to decide"
        );
    }

    #[test]
    fn a_simulation_counts_transitions_and_the_judge_calls_a_new_signal_needs() {
        let stores = Stores::in_memory().unwrap();
        let dig = dig("d", Some("Work"));
        let store = stores.dig(&dig.id).unwrap();
        // m1: newsletter cached high, currently Filed → candidate says Spoil (changed)
        // m2: newsletter cached low, currently Filed → no route → Unresolved (changed, Filed→Unresolved)
        // m3: no cached newsletter → needs judge
        for (pid, cached, current) in [
            ("m1", Some(0.98), Disposition::Filed),
            ("m2", Some(0.1), Disposition::Filed),
            ("m3", None, Disposition::Spoil),
        ] {
            let m = message(&dig.id, pid, "news@list.example");
            repo::messages::upsert(&store, &m).unwrap();
            repo::facts::put(
                &store,
                &m.id,
                &Facts {
                    mailing_list: true,
                    ..Default::default()
                },
            )
            .unwrap();
            if let Some(p) = cached {
                repo::signals::put_evaluation(&store, &eval(&m.id, "newsletter", p)).unwrap();
            }
            classify(&store, &m.id, &dig.id, current);
        }
        let mut candidate =
            parse("Mesh: Newsletters v2\nScope:\nWork Digs\nROUTES\nIF newsletter@v1 > .8 → Spoil")
                .unwrap();
        candidate.id = MeshId("mesh_news".into());
        let report =
            simulate(SimulationInput::new(&stores, vec![dig.clone()], &candidate)).unwrap();
        assert_eq!(report.historical_messages, 3);
        assert_eq!(report.changed, 2);
        assert_eq!(report.unchanged, 1);
        assert_eq!(report.new_judge_calls_required, 1);
        assert!(report
            .transitions
            .iter()
            .any(|t| t.from == DispositionKind::Filed
                && t.to == DispositionKind::Spoil
                && t.count == 1));
        assert!(report
            .transitions
            .iter()
            .any(|t| t.from == DispositionKind::Filed
                && t.to == DispositionKind::Unresolved
                && t.count == 1));
        assert!(report.estimated_cost_usd.unwrap() > 0.0);
        // A candidate scoped to another group touches nothing.
        candidate.scope = MeshScope::DigGroup {
            group: "Personal".into(),
        };
        let report = simulate(SimulationInput::new(&stores, vec![dig], &candidate)).unwrap();
        assert_eq!(report.historical_messages, 0);
    }
}
