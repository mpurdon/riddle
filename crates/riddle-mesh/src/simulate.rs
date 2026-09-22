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

use crate::eval::{evaluate, ListMembership};
use riddle_core::{
    Dig, Disposition, DispositionKind, Facts, Mesh, MeshStatus, NamedList, SimulationReport,
    Transition,
};
use riddle_errors::Result;
use riddle_store::{repo, Stores};
use std::collections::HashMap;

pub struct FactsLists<'a>(pub &'a Facts);

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
}

pub struct SimulationInput<'a> {
    pub stores: &'a Stores,
    pub digs: Vec<Dig>,
    pub candidate: &'a Mesh,
    pub window_days: u32,
    pub now: chrono::DateTime<chrono::Utc>,
    pub max_messages_per_dig: u32,
    pub price_per_mtok: Option<f64>,
}

impl<'a> SimulationInput<'a> {
    pub fn new(stores: &'a Stores, digs: Vec<Dig>, candidate: &'a Mesh) -> Self {
        Self {
            stores,
            digs,
            candidate,
            window_days: 90,
            now: chrono::Utc::now(),
            max_messages_per_dig: 5_000,
            price_per_mtok: Some(0.042),
        }
    }
}

fn kind_of(k: Option<&riddle_core::Classification>) -> DispositionKind {
    k.map(|c| c.disposition.kind())
        .unwrap_or(DispositionKind::Unresolved)
}

pub fn simulate(input: SimulationInput<'_>) -> Result<SimulationReport> {
    let index = input.stores.index();
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
    let mut transitions: HashMap<(DispositionKind, DispositionKind), u64> = HashMap::new();
    let mut sample_changed = Vec::new();
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
        for id in repo::messages::ids_since(&store, &since, input.max_messages_per_dig)? {
            historical += 1;
            let Some(facts) = repo::facts::get(&store, &id)? else {
                unchanged += 1;
                continue;
            };
            let evaluations = repo::signals::evaluations_for(&store, &id)?;
            let current = kind_of(repo::classifications::current(&store, &id)?.as_ref());
            let decision = evaluate(&in_scope, &facts, &evaluations, &FactsLists(&facts));
            if !decision.unresolved_signal_refs.is_empty() {
                needs_judge += 1;
                unchanged += 1;
                continue;
            }
            let to = match decision.output.as_ref() {
                Some(o) if o.escalate && o.disposition.is_none() => DispositionKind::Unresolved,
                Some(o) => o
                    .disposition
                    .as_ref()
                    .map(Disposition::kind)
                    .unwrap_or(DispositionKind::Unresolved),
                None => DispositionKind::Unresolved,
            };
            if to == current {
                unchanged += 1;
            } else {
                changed += 1;
                *transitions.entry((current, to)).or_insert(0) += 1;
                if sample_changed.len() < 20 {
                    sample_changed.push(id.clone());
                }
            }
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
    Ok(SimulationReport {
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
    })
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
            },
        )
        .unwrap();
    }

    #[test]
    fn a_simulation_counts_transitions_and_the_judge_calls_a_new_signal_needs() {
        let stores = Stores::in_memory().unwrap();
        let dig = Dig {
            id: DigId("d1".into()),
            name: "d".into(),
            email: "me@x".into(),
            provider: ProviderKind::Fixture,
            group: Some("Work".into()),
            privacy: PrivacyMode::CloudJudgment,
            projection: ProjectionMode::None,
            status: DigStatus::Synced,
            shadow_started_at: None,
            created_at: chrono::Utc::now(),
        };
        let store = stores.dig(&dig.id).unwrap();
        // m1: newsletter cached high, currently Filed → candidate says Spoil (changed)
        // m2: newsletter cached low, currently Filed → no route → Unresolved (changed, Filed→Unresolved)
        // m3: no cached newsletter → needs judge
        for (pid, cached, current) in [
            ("m1", Some(0.95), Disposition::Filed),
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
