//! The Mesh evaluator: scope resolution and guards exactly as §36 specifies.
//!
//! 1. Meshes are ordered most specific scope first (Dig, group, all), then
//!    by explicit priority, then by name.
//! 2. Routes run in that order; the first definite match wins. A condition
//!    that depends on a Signal with no evaluation is *unknown*: it neither
//!    matches nor lets the rest of that Mesh's `ELSE` chain fire, and the
//!    missing references are reported so Sift can say why the message is
//!    Unresolved.
//! 3. Every guard from every scope runs on the routed result. A veto makes
//!    the disposition Unresolved and is recorded by Mesh and text. A guard
//!    only vetoes its own disposition kind, so `NEVER Spoil` does nothing to
//!    a Find.
//!
//! `sender IN <list>` goes through `ListMembership` so the evaluator never
//! reads the profile itself; Sift and the simulator supply the lists.

use riddle_core::{
    CmpOp, Disposition, DispositionKind, Expr, FactValue, Facts, Mesh, MeshId, MeshOutput,
    MeshStatus, NamedList, SignalEvaluation, UserProfile,
};
use serde::{Deserialize, Serialize};

pub trait ListMembership {
    fn is_member(&self, list: NamedList) -> bool;
}

/// Lists from the User Profile plus the People facts already computed.
pub struct ProfileLists<'a> {
    pub profile: &'a UserProfile,
    pub sender: &'a str,
    pub facts: &'a Facts,
}

impl ListMembership for ProfileLists<'_> {
    fn is_member(&self, list: NamedList) -> bool {
        match list {
            NamedList::NeverHidden => {
                self.facts.sender_in_never_hidden || self.profile.is_never_hidden(self.sender)
            }
            NamedList::Contacts => {
                self.facts.sender_in_contacts || self.facts.sender_replied_to_before
            }
            NamedList::Vips => self.facts.sender_is_vip || self.profile.is_vip(self.sender),
            NamedList::Organisation => {
                self.facts.same_organisation || self.profile.is_own_organisation(self.sender)
            }
            NamedList::Family => self.profile.is_family(self.sender),
        }
    }
}

/// No lists at all; for tests that do not care.
pub struct NoLists;
impl ListMembership for NoLists {
    fn is_member(&self, _list: NamedList) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    fn and(self, other: Truth) -> Truth {
        match (self, other) {
            (Truth::False, _) | (_, Truth::False) => Truth::False,
            (Truth::True, Truth::True) => Truth::True,
            _ => Truth::Unknown,
        }
    }
    fn or(self, other: Truth) -> Truth {
        match (self, other) {
            (Truth::True, _) | (_, Truth::True) => Truth::True,
            (Truth::False, Truth::False) => Truth::False,
            _ => Truth::Unknown,
        }
    }
    fn not(self) -> Truth {
        match self {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Outcome {
    truth: Truth,
    /// Distance of contributing Signal scores from their thresholds; the
    /// confidence policy reads the weakest one.
    margin: Option<f64>,
}

fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) => x,
        (None, y) => y,
    }
}

fn compare_fact(actual: &FactValue, op: CmpOp, expected: &FactValue) -> Truth {
    let holds = match (actual, expected) {
        (FactValue::Bool(a), FactValue::Bool(b)) => op.holds_f64(*a as u8 as f64, *b as u8 as f64),
        (FactValue::Bool(a), FactValue::Number(n)) => op.holds_f64(*a as u8 as f64, *n),
        (FactValue::Number(a), FactValue::Number(n)) => op.holds_f64(*a, *n),
        (FactValue::Number(a), FactValue::Bool(b)) => op.holds_f64(*a, *b as u8 as f64),
        (FactValue::Text(a), FactValue::Text(b)) => match op {
            CmpOp::Eq => a.eq_ignore_ascii_case(b),
            CmpOp::Neq => !a.eq_ignore_ascii_case(b),
            _ => return Truth::False,
        },
        (FactValue::Text(a), FactValue::Bool(b)) => match op {
            CmpOp::Eq => a.is_empty() != *b,
            CmpOp::Neq => a.is_empty() == *b,
            _ => return Truth::False,
        },
        (FactValue::Text(_), FactValue::Number(_))
        | (FactValue::Bool(_), FactValue::Text(_))
        | (FactValue::Number(_), FactValue::Text(_)) => false,
    };
    if holds {
        Truth::True
    } else {
        Truth::False
    }
}

fn eval_expr(
    e: &Expr,
    facts: &Facts,
    evaluations: &[SignalEvaluation],
    lists: &dyn ListMembership,
    unresolved: &mut Vec<String>,
) -> Outcome {
    match e {
        Expr::Always => Outcome {
            truth: Truth::True,
            margin: None,
        },
        Expr::Signal {
            signal,
            version,
            choice,
            op,
            threshold,
        } => {
            let Some(ev) = evaluations
                .iter()
                .find(|x| &x.signal_name == signal && x.signal_version == *version)
            else {
                let key = format!("{signal}@v{version}");
                if !unresolved.contains(&key) {
                    unresolved.push(key);
                }
                return Outcome {
                    truth: Truth::Unknown,
                    margin: None,
                };
            };
            let score = ev.score(choice.as_deref());
            let truth = if op.holds_f64(score, *threshold) {
                Truth::True
            } else {
                Truth::False
            };
            Outcome {
                truth,
                margin: Some((score - threshold).abs()),
            }
        }
        Expr::Fact { fact, op, value } => match facts.get(fact) {
            Some(actual) => Outcome {
                truth: compare_fact(&actual, *op, value),
                margin: None,
            },
            None => Outcome {
                truth: Truth::False,
                margin: None,
            },
        },
        Expr::InList { list, .. } => Outcome {
            truth: if lists.is_member(*list) {
                Truth::True
            } else {
                Truth::False
            },
            margin: None,
        },
        Expr::And { items } => {
            let mut truth = Truth::True;
            let mut margin = None;
            for i in items {
                let o = eval_expr(i, facts, evaluations, lists, unresolved);
                truth = truth.and(o.truth);
                margin = min_opt(margin, o.margin);
            }
            Outcome { truth, margin }
        }
        Expr::Or { items } => {
            let mut truth = Truth::False;
            let mut best: Option<f64> = None;
            let mut any_margin: Option<f64> = None;
            for i in items {
                let o = eval_expr(i, facts, evaluations, lists, unresolved);
                truth = truth.or(o.truth);
                if o.truth == Truth::True {
                    best = match (best, o.margin) {
                        (Some(b), Some(m)) => Some(b.max(m)),
                        (None, m) => m,
                        (b, None) => b,
                    };
                }
                any_margin = min_opt(any_margin, o.margin);
            }
            Outcome {
                truth,
                margin: if truth == Truth::True {
                    best
                } else {
                    any_margin
                },
            }
        }
        Expr::Not { item } => {
            let o = eval_expr(item, facts, evaluations, lists, unresolved);
            Outcome {
                truth: o.truth.not(),
                margin: o.margin,
            }
        }
    }
}

/// Evaluate one expression to a truth value, for tools and tests.
pub fn truth_of(
    e: &Expr,
    facts: &Facts,
    evaluations: &[SignalEvaluation],
    lists: &dyn ListMembership,
) -> Truth {
    let mut unresolved = Vec::new();
    eval_expr(e, facts, evaluations, lists, &mut unresolved).truth
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MeshDecision {
    /// The routed output, with the disposition replaced by Unresolved when
    /// a guard vetoed it. `None` when no route matched.
    pub output: Option<MeshOutput>,
    pub matched_rule: Option<String>,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    pub mesh_status: Option<MeshStatus>,
    /// Vetoes as "Mesh vN: NEVER Spoil IF ...".
    pub vetoes: Vec<String>,
    /// Signal references that had no evaluation, in the order met.
    pub unresolved_signal_refs: Vec<String>,
    /// Weakest Signal margin on the winning route; `None` for fact-only routes.
    pub margin: Option<f64>,
    /// Meshes whose route chain was poisoned by an unknown condition.
    pub unknown_meshes: Vec<String>,
}

impl MeshDecision {
    pub fn disposition(&self) -> Option<&Disposition> {
        self.output.as_ref().and_then(|o| o.disposition.as_ref())
    }
    pub fn vetoed(&self) -> bool {
        !self.vetoes.is_empty()
    }
}

/// Order for resolution: most specific scope first, then priority, then name.
pub fn ordered(meshes: &[Mesh]) -> Vec<&Mesh> {
    let mut v: Vec<&Mesh> = meshes.iter().collect();
    v.sort_by(|a, b| {
        b.scope
            .specificity()
            .cmp(&a.scope.specificity())
            .then(a.priority.cmp(&b.priority))
            .then(a.name.cmp(&b.name))
    });
    v
}

pub fn evaluate(
    meshes: &[Mesh],
    facts: &Facts,
    evaluations: &[SignalEvaluation],
    lists: &dyn ListMembership,
) -> MeshDecision {
    let mut decision = MeshDecision::default();
    let order = ordered(meshes);
    'meshes: for m in &order {
        for r in &m.routes {
            let o = eval_expr(
                &r.condition,
                facts,
                evaluations,
                lists,
                &mut decision.unresolved_signal_refs,
            );
            match o.truth {
                Truth::True => {
                    decision.output = Some(r.output.clone());
                    decision.matched_rule = Some(r.text.clone());
                    decision.mesh_id = Some(m.id.clone());
                    decision.mesh_version = Some(m.version);
                    decision.mesh_name = Some(m.name.clone());
                    decision.mesh_status = Some(m.status);
                    decision.margin = o.margin;
                    break 'meshes;
                }
                Truth::False => continue,
                Truth::Unknown => {
                    // The rest of this chain cannot be trusted; try the next Mesh.
                    decision.unknown_meshes.push(m.key());
                    continue 'meshes;
                }
            }
        }
    }
    // Guards: every scope, on the routed disposition.
    if let Some(kind) = decision.disposition().map(|d| d.kind()) {
        let mut vetoes = Vec::new();
        for m in &order {
            for g in &m.guards {
                if g.never != kind {
                    continue;
                }
                let mut scratch = Vec::new();
                if eval_expr(&g.condition, facts, evaluations, lists, &mut scratch).truth
                    == Truth::True
                {
                    vetoes.push(format!("{}: {}", m.key(), g.text));
                }
            }
        }
        if !vetoes.is_empty() {
            if let Some(o) = decision.output.as_mut() {
                o.disposition = Some(Disposition::Unresolved);
            }
            decision.vetoes = vetoes;
        }
    }
    decision
}

/// The part of a Mesh that needs no Signal: the leading routes without
/// Signal references (the chain stops at the first that has one, since an
/// `ELSE` below it would be meaningless) and the Signal-free guards. This is
/// what runs when the judge is unavailable (§18 fallback).
pub fn deterministic_projection(m: &Mesh) -> Mesh {
    let mut out = m.clone();
    let mut routes = Vec::new();
    for r in &m.routes {
        let mut refs = Vec::new();
        r.condition.signal_refs(&mut refs);
        if !refs.is_empty() {
            break;
        }
        routes.push(r.clone());
    }
    out.routes = routes;
    out.guards.retain(|g| {
        let mut refs = Vec::new();
        g.condition.signal_refs(&mut refs);
        refs.is_empty()
    });
    out
}

pub fn kind_name(k: DispositionKind) -> &'static str {
    k.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use riddle_core::{DigId, Distribution, FindType, MeshScope, MessageId};

    fn ev(name: &str, version: u32, yes: f64) -> SignalEvaluation {
        let mut d = Distribution::new();
        d.insert("YES".into(), yes);
        d.insert("NO".into(), 1.0 - yes);
        SignalEvaluation {
            message_id: MessageId("m".into()),
            signal_name: name.into(),
            signal_version: version,
            engine_identity: "t".into(),
            selected: if yes >= 0.5 {
                "YES".into()
            } else {
                "NO".into()
            },
            distribution: d.clone(),
            calibrated: d,
            confidence: yes.max(1.0 - yes),
            latency_ms: 0,
            input_tokens: 0,
            cached: false,
            deterministic: true,
            context_hash: None,
            request_hash: None,
            evaluated_at: chrono::Utc::now(),
        }
    }

    fn mesh(text: &str, scope: MeshScope, priority: i32) -> Mesh {
        let mut m = parse(text).unwrap();
        m.scope = scope;
        m.priority = priority;
        m.status = MeshStatus::Active;
        m
    }

    #[test]
    fn the_first_matching_route_wins_and_the_margin_is_the_weakest_signal() {
        let m = mesh("Mesh: G\nROUTES\nIF review_requested@v1 > .80 → Find: Review\nELSE IF ci_failure@v1 > .85 AND personally_relevant@v1 > .70 → Find: Do\nELSE → Escalate", MeshScope::AllDigs, 0);
        let d = evaluate(
            std::slice::from_ref(&m),
            &Facts::default(),
            &[
                ev("review_requested", 1, 0.1),
                ev("ci_failure", 1, 0.9),
                ev("personally_relevant", 1, 0.75),
            ],
            &NoLists,
        );
        assert_eq!(
            d.disposition(),
            Some(&Disposition::Find {
                find_type: FindType::Do
            })
        );
        assert!((d.margin.unwrap() - 0.05).abs() < 1e-9);
        assert_eq!(d.mesh_name.as_deref(), Some("G"));
        let d = evaluate(
            &[m],
            &Facts::default(),
            &[
                ev("review_requested", 1, 0.1),
                ev("ci_failure", 1, 0.1),
                ev("personally_relevant", 1, 0.75),
            ],
            &NoLists,
        );
        assert!(d.output.unwrap().escalate);
    }

    #[test]
    fn a_missing_signal_makes_the_chain_unknown_so_else_cannot_fire() {
        let m = mesh(
            "Mesh: G\nROUTES\nIF routine@v1 > .9 → Filed\nELSE → Spoil",
            MeshScope::AllDigs,
            0,
        );
        let d = evaluate(&[m], &Facts::default(), &[], &NoLists);
        assert!(d.output.is_none());
        assert_eq!(d.unresolved_signal_refs, vec!["routine@v1".to_string()]);
        assert_eq!(d.unknown_meshes.len(), 1);
    }

    #[test]
    fn a_dig_scoped_mesh_overrides_a_group_one_and_a_global_guard_still_vetoes() {
        let global = mesh("Mesh: System\nROUTES\nIF has_calendar_part → Find: Attend\nGUARDS\nNEVER Spoil IF sender IN never_hidden", MeshScope::AllDigs, 0);
        let group = mesh(
            "Mesh: Work newsletters\nROUTES\nIF newsletter@v1 > .8 → Filed + Stratum: Newsletters",
            MeshScope::DigGroup {
                group: "Work".into(),
            },
            0,
        );
        let dig = mesh(
            "Mesh: This dig\nROUTES\nIF newsletter@v1 > .8 → Spoil",
            MeshScope::Dig {
                dig_id: DigId("d1".into()),
            },
            0,
        );
        let evals = [ev("newsletter", 1, 0.95)];
        struct NeverHidden;
        impl ListMembership for NeverHidden {
            fn is_member(&self, list: NamedList) -> bool {
                list == NamedList::NeverHidden
            }
        }
        let d = evaluate(
            &[global.clone(), group.clone(), dig.clone()],
            &Facts::default(),
            &evals,
            &NoLists,
        );
        assert_eq!(
            d.disposition(),
            Some(&Disposition::Spoil),
            "the Dig scope wins"
        );
        assert_eq!(d.mesh_name.as_deref(), Some("This dig"));
        let d = evaluate(
            &[global, group, dig],
            &Facts::default(),
            &evals,
            &NeverHidden,
        );
        assert_eq!(
            d.disposition(),
            Some(&Disposition::Unresolved),
            "the global guard vetoes the Dig route"
        );
        assert_eq!(d.vetoes.len(), 1);
        assert!(d.vetoes[0].starts_with("System v1: NEVER Spoil"));
    }

    #[test]
    fn a_never_spoil_guard_does_nothing_to_a_find() {
        let m = mesh("Mesh: G\nROUTES\nIF review_requested@v1 > .8 → Find: Review\nGUARDS\nNEVER Spoil IF sender IN never_hidden", MeshScope::AllDigs, 0);
        struct All;
        impl ListMembership for All {
            fn is_member(&self, _: NamedList) -> bool {
                true
            }
        }
        let d = evaluate(
            &[m],
            &Facts::default(),
            &[ev("review_requested", 1, 0.96)],
            &All,
        );
        assert_eq!(
            d.disposition(),
            Some(&Disposition::Find {
                find_type: FindType::Review
            })
        );
        assert!(!d.vetoed());
    }

    #[test]
    fn priority_then_name_orders_meshes_within_one_scope() {
        let a = mesh(
            "Mesh: B second\nROUTES\nIF mailing_list → Filed",
            MeshScope::AllDigs,
            5,
        );
        let b = mesh(
            "Mesh: A first\nROUTES\nIF mailing_list → Spoil",
            MeshScope::AllDigs,
            5,
        );
        let c = mesh(
            "Mesh: Z urgent\nROUTES\nIF mailing_list → Find: Know",
            MeshScope::AllDigs,
            1,
        );
        let facts = Facts {
            mailing_list: true,
            ..Default::default()
        };
        let d = evaluate(&[a, b, c], &facts, &[], &NoLists);
        assert_eq!(d.mesh_name.as_deref(), Some("Z urgent"));
    }

    #[test]
    fn the_deterministic_projection_keeps_only_signal_free_leading_routes() {
        let m = mesh("Mesh: G\nROUTES\nIF gmail_spam → Contamination\nELSE IF routine@v1 > .9 → Spoil\nELSE → Filed\nGUARDS\nNEVER Spoil IF sender IN vips\nNEVER Spoil IF security_alert@v1 > .8", MeshScope::AllDigs, 0);
        let p = deterministic_projection(&m);
        assert_eq!(p.routes.len(), 1);
        assert_eq!(p.guards.len(), 1);
    }
}
