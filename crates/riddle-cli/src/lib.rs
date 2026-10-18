//! What `riddle decide --preview` and `riddle trace` say to a person on
//! stderr, beside the JSON they print on stdout for a script (17 Oct 2026).
//! The words are built here from the stored structures with a fixed
//! vocabulary, never by a model, so two runs over the same trace read the
//! same, and so the formatting can be tested without a store.

use riddle_app::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;

/// A message named by a link copied from its page, which carries the Dig,
/// or by an id with `--dig`.
pub fn message_ref(dig: Option<&str>, message: &str) -> Result<(DigId, MessageId)> {
    if let Some(pair) = parse_message_link(message) {
        return Ok(pair);
    }
    match dig {
        Some(d) if !d.is_empty() => Ok((DigId::from(d), MessageId::from(message))),
        _ => Err(RiddleError::invalid(
            "name the message by a riddle://message/… link, or give its id with --dig",
        )),
    }
}

/// The trace behind a message's current decision (the Shadow proposal's
/// when that is all there is).
fn trace_of_message(app: &App, dig: &DigId, message: &MessageId) -> Result<DecisionTrace> {
    let store = app.stores.dig(dig)?;
    let k = repo::classifications::current(&store, message)?
        .or(repo::classifications::current_shadow(&store, message)?)
        .ok_or_else(|| RiddleError::not_found("the message has not been sifted yet"))?;
    repo::traces::get(&store, &k.trace_id)?
        .ok_or_else(|| RiddleError::not_found(format!("trace {}", k.trace_id)))
}

/// A stored trace by its id (searching every Dig without `--dig`), or by
/// a message link or id, which gives its current decision's trace.
pub fn find_trace(app: &App, dig: Option<&str>, id: &str) -> Result<DecisionTrace> {
    if let Some((d, m)) = parse_message_link(id) {
        return trace_of_message(app, &d, &m);
    }
    let digs: Vec<DigId> = match dig {
        Some(d) if !d.is_empty() => vec![DigId::from(d)],
        _ => app.list_all_digs()?.into_iter().map(|d| d.id).collect(),
    };
    for d in &digs {
        let store = app.stores.dig(d)?;
        if let Some(t) = repo::traces::get(&store, &TraceId::from(id))? {
            return Ok(t);
        }
    }
    match dig {
        Some(d) if !d.is_empty() => trace_of_message(app, &DigId::from(d), &MessageId::from(id)),
        _ => Err(RiddleError::not_found(format!(
            "no trace {id}; for a message id, give its Dig with --dig"
        ))),
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
        None => String::new(),
    }
}

fn disposition_words(d: &Disposition) -> String {
    match d {
        Disposition::Find { find_type } => format!("Find: {}", find_type.label()),
        Disposition::Filed => "Filed".into(),
        Disposition::Spoil => "Spoil".into(),
        Disposition::Contamination => "Contamination".into(),
        Disposition::Unresolved => "Unresolved".into(),
    }
}

fn two_places(n: f64) -> String {
    let s = format!("{n:.2}");
    s.strip_prefix('0').map(str::to_string).unwrap_or(s)
}

/// The preview for a person: the diff, the Signals' provenance, what each
/// earlier Mesh made of it, and that nothing was written.
pub fn format_preview(p: &DecisionPreview) -> String {
    let mut out = format!("{}.", capitalised(&p.diff));
    if !p.note.is_empty() {
        out.push_str(&format!("\nSignals: {}.", p.note));
    }
    let earlier = p.would_trace.earlier_meshes();
    if !earlier.is_empty() {
        out.push_str("\nOther Meshes that ran:");
        for line in earlier {
            out.push_str(&format!("\n  {line}"));
        }
    }
    out.push_str("\nNothing was written; the stored decision stands.");
    out
}

/// A stored trace for a person, one section per line: who decided, the
/// rule, every Mesh's outcome, the list clauses as they read, the policy,
/// the Signals, the escalation, the timing and the build.
pub fn format_trace(t: &DecisionTrace) -> String {
    let mut lines = Vec::new();
    let mut head = format!(
        "Trace {} · message {} · dig {}",
        t.id, t.message_id, t.dig_id
    );
    if !t.pass_id.is_empty() {
        head.push_str(&format!(" · pass {}", t.pass_id));
    }
    if t.shadow {
        head.push_str(" · shadow (recorded, not applied)");
    }
    lines.push(head);
    let mut decided = format!(
        "Decided: {} by {:?}",
        disposition_words(&t.conclusion),
        t.mechanism
    );
    match (&t.mesh_name, t.mesh_version) {
        (Some(name), Some(v)) => decided.push_str(&format!(" · {name} v{v}")),
        _ => decided.push_str(" · no Mesh"),
    }
    if let Some(c) = t.confidence {
        decided.push_str(&format!(" · confidence {}", two_places(c)));
    }
    if let Some(m) = t.margin {
        decided.push_str(&format!(" · margin {}", two_places(m)));
    }
    lines.push(decided);
    if let Some(r) = &t.matched_rule {
        lines.push(format!("Rule: {r}"));
    }
    if let Some(g) = &t.gate {
        lines.push(format!("Gate: {g}"));
    }
    if !t.guard_vetoes.is_empty() {
        lines.push(format!("Guard vetoes: {}", t.guard_vetoes.join("; ")));
    }
    if !t.mesh_outcomes.is_empty() {
        lines.push("Meshes, in the order they ran:".into());
        for o in &t.mesh_outcomes {
            lines.push(format!("  {}", o.line()));
        }
    }
    if !t.list_results.is_empty() {
        lines.push("Lists, as they read then:".into());
        for l in &t.list_results {
            lines.push(format!(
                "  {} · {} · {}",
                l.list,
                l.value,
                if l.matched { "on it" } else { "not on it" }
            ));
        }
    }
    if let Some(p) = &t.policy {
        lines.push(format!(
            "Policy: margins Find {}, Filed {}, Spoil {}, Contamination {}{}",
            two_places(p.margin_find),
            two_places(p.margin_filed),
            two_places(p.margin_spoil),
            two_places(p.margin_contamination),
            if p.escalate_non_english {
                "; non-English mail escalates"
            } else {
                ""
            }
        ));
    }
    if !t.signals.is_empty() {
        lines.push("Signals:".into());
        for s in &t.signals {
            lines.push(format!(
                "  {}@v{} {} {}{}",
                s.signal_name,
                s.signal_version,
                s.selected,
                two_places(s.score(None)),
                if s.cached { " (cached)" } else { "" }
            ));
        }
    }
    if let Some(e) = &t.escalation {
        lines.push(format!(
            "Escalation: {} because {}; {}",
            e.model_id, e.reason, e.rationale
        ));
    }
    lines.push(format!(
        "Timing: {}ms total (facts {}, judge {}, mesh {}, llm {}, persist {})",
        t.timing.total_ms,
        t.timing.facts_ms,
        t.timing.judge_ms,
        t.timing.mesh_ms,
        t.timing.llm_ms,
        t.timing.persist_ms
    ));
    lines.push(format!("Build: {} ({})", t.build_stamp, t.app_version));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classification(d: Disposition, mesh: Option<(&str, u32)>) -> Classification {
        Classification {
            id: ClassificationId::new(),
            message_id: MessageId::from("m1"),
            dig_id: DigId::from("d1"),
            disposition: d,
            strata: vec![],
            notify: false,
            mechanism: DecisionMechanism::Mesh,
            mesh_id: mesh.map(|(id, _)| MeshId::from(id)),
            mesh_version: mesh.map(|(_, v)| v),
            matched_rule: Some(
                "IF payment_related@v1 > .70\n    → Filed + Stratum: Finance".into(),
            ),
            confidence: Some(0.82),
            reason: String::new(),
            trace_id: TraceId::from("tr_1"),
            shadow: false,
            created_at: chrono::Utc::now(),
            superseded_at: None,
            series: None,
            importance: Importance::Normal,
        }
    }

    fn trace(k: &Classification, name: &str) -> DecisionTrace {
        DecisionTrace {
            id: k.trace_id.clone(),
            message_id: k.message_id.clone(),
            dig_id: k.dig_id.clone(),
            objective: String::new(),
            strategy: "mesh over judge signals".into(),
            facts: Facts::default(),
            gate: None,
            signals: vec![],
            mesh_id: k.mesh_id.clone(),
            mesh_version: k.mesh_version,
            mesh_name: Some(name.into()),
            matched_rule: k.matched_rule.clone(),
            guard_vetoes: vec![],
            escalation: None,
            conclusion: k.disposition.clone(),
            confidence: k.confidence,
            mechanism: k.mechanism,
            timing: TraceTiming {
                total_ms: 120,
                judge_ms: 80,
                ..Default::default()
            },
            cost: TraceCost::default(),
            judge_identity: None,
            app_version: "0.1.0".into(),
            build_stamp: "abc123@2026-10-17".into(),
            shadow: false,
            created_at: k.created_at,
            pass_id: "pass-7".into(),
            list_results: vec![ListClauseResult {
                list: "list:banks".into(),
                value: "billing@bank.example".into(),
                matched: true,
            }],
            policy: Some(PolicyThresholds {
                margin_find: 0.05,
                margin_spoil: 0.15,
                margin_filed: 0.05,
                margin_contamination: 0.25,
                escalate_non_english: true,
            }),
            margin: Some(0.12),
            mesh_outcomes: vec![
                MeshOutcome {
                    mesh_id: MeshId::from("promo"),
                    version: 2,
                    name: "Frequent promotions".into(),
                    outcome: MeshOutcomeKind::MarginShortfall {
                        route: "IF marketing@v1 > .70 → Spoil".into(),
                        signal: Some("marketing@v1".into()),
                        score: Some(0.78),
                        needed_score: Some(0.85),
                        margin: 0.08,
                        needed: 0.15,
                    },
                },
                MeshOutcome {
                    mesh_id: MeshId::from("bills"),
                    version: 4,
                    name: "Bills".into(),
                    outcome: MeshOutcomeKind::Matched {
                        route: "IF payment_related@v1 > .70 → Filed".into(),
                    },
                },
                MeshOutcome {
                    mesh_id: MeshId::from("updates"),
                    version: 1,
                    name: "Updates".into(),
                    outcome: MeshOutcomeKind::NotReached,
                },
            ],
        }
    }

    #[test]
    fn a_preview_says_the_diff_the_signals_and_that_nothing_was_written() {
        let stored = classification(Disposition::Spoil, Some(("promo", 2)));
        let would = classification(Disposition::Filed, Some(("bills", 4)));
        let p = DecisionPreview {
            message_id: MessageId::from("m1"),
            dig_id: DigId::from("d1"),
            stored: Some(stored.clone()),
            stored_trace: Some(trace(&stored, "Frequent promotions")),
            would: would.clone(),
            would_trace: trace(&would, "Bills"),
            changed: true,
            diff: "would be Filed by Bills v4 instead of Spoil by Frequent promotions v2".into(),
            note: "3 matched on cached Signals, 1 needed a fresh evaluation".into(),
        };
        let text = format_preview(&p);
        assert_eq!(
            text,
            "Would be Filed by Bills v4 instead of Spoil by Frequent promotions v2.\n\
             Signals: 3 matched on cached Signals, 1 needed a fresh evaluation.\n\
             Other Meshes that ran:\n  Frequent promotions v2: marketing .78, needed .85\n\
             Nothing was written; the stored decision stands."
        );
    }

    #[test]
    fn a_trace_prints_the_decider_every_mesh_the_lists_and_the_policy() {
        let k = classification(Disposition::Filed, Some(("bills", 4)));
        let text = format_trace(&trace(&k, "Bills"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Trace tr_1 · message m1 · dig d1 · pass pass-7");
        assert_eq!(
            lines[1],
            "Decided: Filed by Mesh · Bills v4 · confidence .82 · margin .12"
        );
        assert!(text.contains("Meshes, in the order they ran:\n  Frequent promotions v2: marketing .78, needed .85\n  Bills v4: decided\n  Updates v1: not reached"));
        assert!(
            text.contains("Lists, as they read then:\n  list:banks · billing@bank.example · on it")
        );
        assert!(text.contains(
            "Policy: margins Find .05, Filed .05, Spoil .15, Contamination .25; non-English mail escalates"
        ));
        assert!(text.contains("Timing: 120ms total (facts 0, judge 80, mesh 0, llm 0, persist 0)"));
        assert!(text.ends_with("Build: abc123@2026-10-17 (0.1.0)"));
    }

    #[test]
    fn a_message_is_named_by_its_link_or_by_its_id_with_a_dig() {
        let (d, m) = message_ref(None, "riddle://message/d1/m1 (Sender · Subject)").unwrap();
        assert_eq!((d.as_str(), m.as_str()), ("d1", "m1"));
        let (d, m) = message_ref(Some("d2"), "m2").unwrap();
        assert_eq!((d.as_str(), m.as_str()), ("d2", "m2"));
        assert!(message_ref(None, "m3").is_err());
    }
}
