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

use crate::policy::ConfidencePolicy;
use riddle_core::{
    CmpOp, Disposition, DispositionKind, Expr, FactValue, Facts, Mesh, MeshId, MeshOutput,
    MeshStatus, NamedList, SignalEvaluation, UserProfile,
};
use serde::{Deserialize, Serialize};

pub trait ListMembership {
    fn is_member(&self, list: NamedList) -> bool;
    /// Whether the sender is on one of the user's named lists
    /// (`sender IN list:banks`); false for a list that does not exist.
    fn in_named_list(&self, _list: &str) -> bool {
        false
    }
}

/// Lists from the User Profile plus the People facts already computed.
pub struct ProfileLists<'a> {
    pub profile: &'a UserProfile,
    pub sender: &'a str,
    pub facts: &'a Facts,
    pub named: &'a riddle_core::NamedLists,
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
    fn in_named_list(&self, list: &str) -> bool {
        self.named.contains(list, self.sender)
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
            CmpOp::Contains => a.to_lowercase().contains(&b.to_lowercase()),
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

/// A missing Signal counts as unresolved only when it left the answer
/// undecided; the And and Or arms share this rule.
fn report_missing(truth: Truth, missing: Vec<String>, unresolved: &mut Vec<String>) {
    if truth != Truth::Unknown {
        return;
    }
    for m in missing {
        if !unresolved.contains(&m) {
            unresolved.push(m);
        }
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
        Expr::InNamedList { list, .. } => Outcome {
            truth: if lists.in_named_list(list) {
                Truth::True
            } else {
                Truth::False
            },
            margin: None,
        },
        Expr::InList { list, .. } => Outcome {
            truth: if lists.is_member(*list) {
                Truth::True
            } else {
                Truth::False
            },
            margin: None,
        },
        // A missing Signal is only reported when it leaves the answer
        // undecided: `sender_domain = github.com AND ci_failure@v1 > .85`
        // on mail from elsewhere is simply false. Sift no longer scores
        // Signals a message's facts rule out, so reporting them regardless
        // made that mail look like it needed the judge.
        Expr::And { items } => {
            let mut truth = Truth::True;
            let mut margin = None;
            let mut missing = Vec::new();
            for i in items {
                let o = eval_expr(i, facts, evaluations, lists, &mut missing);
                truth = truth.and(o.truth);
                margin = min_opt(margin, o.margin);
            }
            report_missing(truth, missing, unresolved);
            Outcome { truth, margin }
        }
        Expr::Or { items } => {
            let mut truth = Truth::False;
            let mut best: Option<f64> = None;
            let mut any_margin: Option<f64> = None;
            let mut missing = Vec::new();
            for i in items {
                let o = eval_expr(i, facts, evaluations, lists, &mut missing);
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
            report_missing(truth, missing, unresolved);
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
    /// The matched route read against this message, in words.
    pub words: Option<String>,
    pub mesh_id: Option<MeshId>,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    pub mesh_status: Option<MeshStatus>,
    /// Vetoes as "Mesh vN: NEVER Spoil IF ...".
    pub vetoes: Vec<String>,
    /// The same vetoes in words: "Mesh vN says never Spoil when ...".
    pub veto_words: Vec<String>,
    /// Signal references that had no evaluation, in the order met.
    pub unresolved_signal_refs: Vec<String>,
    /// Weakest Signal margin on the winning route; `None` for fact-only routes.
    pub margin: Option<f64>,
    /// Meshes whose route chain was poisoned by an unknown condition.
    pub unknown_meshes: Vec<String>,
    /// Routes that matched on a Signal too close to call and so let a later
    /// Mesh decide: "Frequent promotions v2: IF … (margin 0.08 is below the 0.15 required)".
    #[serde(default)]
    pub passed_over: Vec<String>,
    /// Strata from tag routes (`MeshOutput::is_tag`) that matched before the
    /// deciding route, added to whatever is decided, Unresolved included.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl MeshDecision {
    pub fn disposition(&self) -> Option<&Disposition> {
        self.output.as_ref().and_then(|o| o.disposition.as_ref())
    }
    pub fn vetoed(&self) -> bool {
        !self.vetoes.is_empty()
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_lowercase(), chars.as_str()),
        None => String::new(),
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
    evaluate_with(meshes, facts, evaluations, lists, None)
}

/// `evaluate` under Sift's confidence policy. A route that matches on a
/// Signal whose margin is too thin for its disposition does not end the
/// search: the next Mesh gets its turn, and the thin match is the decision
/// only when nothing after it matches. Before this, an early rule that
/// matched a borderline message (a promotion scoring 0.78 against `> .70`)
/// held it Unresolved although a later Mesh would have decided it with
/// confidence: 77 Spoil messages would have gone Unresolved on the mailbox
/// that showed it. A route falls through to the next Mesh, not to its own
/// Mesh's ELSE, which would read the author's chain as something else. A
/// borderline Find never falls through to Spoil or Contamination: it would
/// have been Unresolved, which the person sees, and falling through must not
/// hide it; only a later Find or Filed may take it over.
pub fn evaluate_with(
    meshes: &[Mesh],
    facts: &Facts,
    evaluations: &[SignalEvaluation],
    lists: &dyn ListMembership,
    policy: Option<&ConfidencePolicy>,
) -> MeshDecision {
    let mut unresolved_signal_refs = Vec::new();
    let mut unknown_meshes = Vec::new();
    let mut passed_over = Vec::new();
    // The route that decides, and the first thin match in case none does.
    let mut winner = None;
    let mut held = None;
    let mut tags: Vec<String> = Vec::new();
    let order = ordered(meshes);
    'meshes: for m in &order {
        for r in &m.routes {
            let o = eval_expr(
                &r.condition,
                facts,
                evaluations,
                lists,
                &mut unresolved_signal_refs,
            );
            match o.truth {
                // A tag adds its Strata and lets evaluation go on.
                Truth::True if r.output.is_tag() => {
                    for s in &r.output.strata {
                        if !tags.contains(s) {
                            tags.push(s.clone());
                        }
                    }
                }
                Truth::True => {
                    let thin = policy
                        .zip(r.output.disposition.as_ref())
                        .and_then(|(p, d)| p.shortfall(d.kind(), o.margin));
                    if let Some((margin, needed)) = thin {
                        passed_over.push(format!(
                            "{}: {} (margin {})",
                            m.key(),
                            r.text,
                            crate::policy::shortfall_words(margin, needed)
                        ));
                        held.get_or_insert((*m, r, o.margin));
                        continue 'meshes;
                    }
                    // A borderline Find surfaced as Unresolved, which the
                    // person sees; a later rule may take it over, but not
                    // by hiding it. "Might need your attention" must not
                    // become Spoil or Contamination by falling through.
                    // Not `Disposition::hides`, which counts Filed: Filed
                    // keeps mail in the Inbox view (ARCHITECTURE.md, "Filed
                    // may start Active"), so it may take a borderline Find over.
                    let buries_a_find = held.is_some_and(|(_, h, _)| {
                        matches!(h.output.disposition, Some(Disposition::Find { .. }))
                    }) && matches!(
                        r.output.disposition,
                        Some(Disposition::Spoil | Disposition::Contamination)
                    );
                    if buries_a_find {
                        continue 'meshes;
                    }
                    winner = Some((*m, r, o.margin));
                    break 'meshes;
                }
                Truth::False => continue,
                Truth::Unknown => {
                    // The rest of this chain cannot be trusted; try the next Mesh.
                    unknown_meshes.push(m.key());
                    continue 'meshes;
                }
            }
        }
    }
    // A thin match decides only when nothing after it stood; it is then the
    // decision, held by the policy as before, not something passed over.
    let chosen = match (winner, held) {
        (Some(w), _) => Some(w),
        (None, Some(h)) => {
            passed_over.remove(0);
            Some(h)
        }
        (None, None) => None,
    };
    let mut decision = MeshDecision {
        unresolved_signal_refs,
        unknown_meshes,
        passed_over,
        ..Default::default()
    };
    decision.tags = tags.clone();
    if let Some((m, r, margin)) = chosen {
        let mut output = r.output.clone();
        for t in &tags {
            if !output.strata.contains(t) {
                output.strata.push(t.clone());
            }
        }
        decision.output = Some(output);
        decision.matched_rule = Some(r.text.clone());
        decision.words = Some(crate::humanize::route_decision(
            r,
            &crate::humanize::Evidence { facts, evaluations },
        ));
        decision.mesh_id = Some(m.id.clone());
        decision.mesh_version = Some(m.version);
        decision.mesh_name = Some(m.name.clone());
        decision.mesh_status = Some(m.status);
        decision.margin = margin;
    }
    // Guards: every scope, on the routed disposition.
    if let Some(kind) = decision.disposition().map(|d| d.kind()) {
        let mut vetoes = Vec::new();
        let mut veto_words = Vec::new();
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
                    veto_words.push(format!(
                        "{} says {}",
                        m.key(),
                        lower_first(crate::humanize::guard_reading(g).trim_end_matches('.'))
                    ));
                }
            }
        }
        if !vetoes.is_empty() {
            if let Some(o) = decision.output.as_mut() {
                o.disposition = Some(Disposition::Unresolved);
            }
            decision.vetoes = vetoes;
            decision.veto_words = veto_words;
        }
    }
    decision
}

/// Replay one Mesh's routes against a message for the "Why?" answer: each
/// route's conditions with their actual values, and which route decided.
/// The first route that matches, or that cannot be decided, ends the chain;
/// the rest are `NotReached`.
pub fn check_routes(
    m: &Mesh,
    facts: &Facts,
    evaluations: &[SignalEvaluation],
    lists: &dyn ListMembership,
) -> Vec<riddle_core::RouteCheck> {
    use riddle_core::{ClauseCheck, ClauseResult, RouteCheck, RouteOutcome};
    let result_of = |t: Truth| match t {
        Truth::True => ClauseResult::Pass,
        Truth::False => ClauseResult::Fail,
        Truth::Unknown => ClauseResult::Unknown,
    };
    let actual_of = |e: &Expr| -> Option<String> {
        match e {
            Expr::Signal {
                signal,
                version,
                choice,
                ..
            } => evaluations
                .iter()
                .find(|x| &x.signal_name == signal && x.signal_version == *version)
                .map(|x| format!("{:.2}", x.score(choice.as_deref()))),
            Expr::Fact { fact, .. } => facts.get(fact).map(|v| match v {
                FactValue::Bool(b) => b.to_string(),
                FactValue::Number(n) => format!("{n}"),
                FactValue::Text(t) if t.is_empty() => "(none)".to_string(),
                FactValue::Text(t) => t,
            }),
            _ => None,
        }
    };
    let signal_of = |e: &Expr| -> Option<String> {
        let mut refs = Vec::new();
        e.signal_refs(&mut refs);
        match refs.as_slice() {
            [(name, version)] => Some(format!("{name}@v{version}")),
            _ => None,
        }
    };
    let split = |e: &Expr| -> Vec<Expr> {
        match e {
            Expr::And { items } => items.clone(),
            Expr::Always => vec![],
            other => vec![other.clone()],
        }
    };
    let mut done = false;
    let mut out = Vec::new();
    for r in &m.routes {
        let own = match (&r.when, &r.local) {
            (Some(_), Some(local)) => split(local),
            (Some(_), None) => vec![],
            (None, _) => split(&r.condition),
        };
        let check = |c: &Expr| {
            let mut ignored = Vec::new();
            ClauseCheck {
                text: crate::format::fmt_expr(c),
                actual: actual_of(c),
                result: result_of(eval_expr(c, facts, evaluations, lists, &mut ignored).truth),
                signal: signal_of(c),
            }
        };
        let clauses: Vec<ClauseCheck> = own.iter().map(check).collect();
        let outcome = if done {
            RouteOutcome::NotReached
        } else {
            let mut ignored = Vec::new();
            match eval_expr(&r.condition, facts, evaluations, lists, &mut ignored).truth {
                Truth::True => {
                    done = true;
                    RouteOutcome::Matched
                }
                Truth::Unknown => {
                    done = true;
                    RouteOutcome::Unknown
                }
                Truth::False => RouteOutcome::NoMatch,
            }
        };
        out.push(RouteCheck {
            when: r.when.as_ref().map(check),
            clauses,
            output: crate::format::fmt_output(&r.output),
            outcome,
        });
    }
    out
}

/// The Signals a Mesh could need for this message. A route whose facts
/// already rule it out (`sender_domain = github.com` on mail from anywhere
/// else, a `WHEN` block that does not apply) cannot match whatever its
/// Signals say, so they are not sent to the judge. Before this every Signal
/// of every Mesh was scored for every message.
pub fn signals_needed(m: &Mesh, facts: &Facts, lists: &dyn ListMembership) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    let conditions = m
        .routes
        .iter()
        .map(|r| &r.condition)
        .chain(m.guards.iter().map(|g| &g.condition));
    for c in conditions {
        let mut ignored = Vec::new();
        if eval_expr(c, facts, &[], lists, &mut ignored).truth == Truth::False {
            continue;
        }
        let mut refs = Vec::new();
        c.signal_refs(&mut refs);
        for r in refs {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out
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
    #[test]
    fn the_why_breakdown_shows_each_condition_and_where_the_chain_stopped() {
        use riddle_core::{ClauseResult, RouteOutcome};
        let mesh = crate::parse::parse(GITHUB_BLOCK).unwrap();
        let facts = github("github-actions[bot]", Some("ci_activity"));
        let checks = check_routes(&mesh, &facts, &[ev("ci_failure", 1, 0.05)], &NoLists);
        let outcomes: Vec<_> = checks.iter().map(|c| c.outcome).collect();
        assert_eq!(
            outcomes,
            vec![
                RouteOutcome::NoMatch,
                RouteOutcome::NoMatch,
                RouteOutcome::Matched,
                RouteOutcome::NotReached,
                RouteOutcome::NotReached
            ]
        );
        let when = checks[0].when.as_ref().unwrap();
        assert_eq!(when.text, "sender_domain = github.com");
        assert_eq!(
            (when.actual.as_deref(), when.result),
            (Some("github.com"), ClauseResult::Pass)
        );
        assert_eq!(when.signal, None, "a fact clause names no Signal");
        assert_eq!(
            checks[0].clauses[0].actual.as_deref(),
            Some("github-actions[bot]")
        );
        assert_eq!(checks[0].clauses[0].result, ClauseResult::Fail);
        let ci = &checks[1].clauses[1];
        assert_eq!(ci.text, "ci_failure@v1 > .85");
        assert_eq!(ci.actual.as_deref(), Some("0.05"));
        assert_eq!(ci.result, ClauseResult::Fail);
        assert_eq!(ci.signal.as_deref(), Some("ci_failure@v1"));
        assert_eq!(checks[2].output, "Spoil");
    }

    #[test]
    fn a_when_block_that_does_not_apply_needs_none_of_its_signals() {
        let mesh = crate::parse::parse(GITHUB_BLOCK).unwrap();
        let other = Facts {
            sender_domain: "example.com".into(),
            ..Default::default()
        };
        assert!(signals_needed(&mesh, &other, &NoLists).is_empty());
        let gh = github("github-actions[bot]", Some("ci_activity"));
        assert_eq!(
            signals_needed(&mesh, &gh, &NoLists),
            vec![("ci_failure".to_string(), 1)]
        );
    }

    const GITHUB_BLOCK: &str = "Mesh: GitHub v2\n\nROUTES\n\nWHEN sender_domain = github.com\n    IF sender_name CONTAINS \"dependabot\" → Spoil\n    ELSE IF header X-GitHub-Reason = ci_activity AND ci_failure@v1 > .85 → Find: Do\n    ELSE IF header X-GitHub-Reason = ci_activity → Spoil\n    ELSE → Filed\nEND\n\nELSE IF sender_domain = example.com → Find: Respond\n";

    fn github(name: &str, reason: Option<&str>) -> Facts {
        let mut f = Facts {
            sender_domain: "github.com".into(),
            sender_name: name.into(),
            ..Default::default()
        };
        if let Some(r) = reason {
            f.headers.insert("x-github-reason".into(), r.into());
        }
        f
    }

    fn disposition(mesh: &Mesh, facts: &Facts, evals: &[SignalEvaluation]) -> Option<Disposition> {
        evaluate(std::slice::from_ref(mesh), facts, evals, &NoLists)
            .output
            .and_then(|o| o.disposition)
    }

    /// A route that only adds Strata tags the message and lets evaluation
    /// go on, so a topic (Finance) joins whatever later rule decides, and
    /// stays on mail nothing decides.
    #[test]
    fn a_strata_only_route_tags_and_the_deciding_route_still_decides() {
        let mut topics = crate::parse::parse("Mesh: Money talk\nPriority: -30\nROUTES\nIF finance_topic@v1 > .80\n    → Stratum: Finance").unwrap();
        topics.id = MeshId("mesh_topics".into());
        let mut people = crate::parse::parse("Mesh: Human correspondence\nPriority: -5\nROUTES\nIF known_thread\n    → Find: Know + Stratum: People").unwrap();
        people.id = MeshId("mesh_people".into());
        let meshes = [topics, people];
        let money = [ev("finance_topic", 1, 0.95)];
        let in_thread = Facts {
            known_thread: true,
            ..Default::default()
        };
        let d = evaluate(&meshes, &in_thread, &money, &NoLists);
        let out = d.output.unwrap();
        assert_eq!(
            out.disposition,
            Some(Disposition::Find {
                find_type: FindType::Know
            })
        );
        assert_eq!(
            out.strata,
            vec!["People".to_string(), "Finance".to_string()]
        );
        assert_eq!(
            d.mesh_name.as_deref(),
            Some("Human correspondence"),
            "the tag decided nothing"
        );
        // Nothing decides: the tag is still there for Unresolved mail.
        let d = evaluate(&meshes, &Facts::default(), &money, &NoLists);
        assert!(d.output.is_none());
        assert_eq!(d.tags, vec!["Finance".to_string()]);
    }

    #[test]
    fn a_when_block_routes_github_mail_by_type() {
        let mesh = crate::parse::parse(GITHUB_BLOCK).unwrap();
        assert_eq!(
            disposition(&mesh, &github("dependabot[bot]", None), &[]),
            Some(Disposition::Spoil)
        );
        let failed = vec![ev("ci_failure", 1, 0.95)];
        assert_eq!(
            disposition(
                &mesh,
                &github("github-actions[bot]", Some("ci_activity")),
                &failed
            ),
            Some(Disposition::Find {
                find_type: FindType::Do
            })
        );
        let passed = vec![ev("ci_failure", 1, 0.05)];
        assert_eq!(
            disposition(
                &mesh,
                &github("github-actions[bot]", Some("ci_activity")),
                &passed
            ),
            Some(Disposition::Spoil)
        );
        assert_eq!(
            disposition(&mesh, &github("octocat", Some("mention")), &[]),
            Some(Disposition::Filed),
            "the block ELSE claims the rest of GitHub"
        );
    }

    #[test]
    fn a_when_block_leaves_other_mail_to_the_routes_after_it() {
        let mesh = crate::parse::parse(GITHUB_BLOCK).unwrap();
        // No ci_failure evaluation: for non-GitHub mail the block's Signal is
        // never needed, so it must not hold up the rest of the chain.
        let other = Facts {
            sender_domain: "example.com".into(),
            ..Default::default()
        };
        assert_eq!(
            disposition(&mesh, &other, &[]),
            Some(Disposition::Find {
                find_type: FindType::Respond
            })
        );
        let stranger = Facts {
            sender_domain: "elsewhere.org".into(),
            ..Default::default()
        };
        assert_eq!(disposition(&mesh, &stranger, &[]), None);
    }

    #[test]
    fn a_github_rule_can_tell_dependabot_and_ci_passes_apart_without_a_signal() {
        let mesh = crate::parse::parse(
            "Mesh: GitHub v1\n\nROUTES\n\nIF sender_name CONTAINS \"dependabot\" → Spoil\n\nELSE IF header X-GitHub-Reason = ci_activity → Filed\n",
        )
        .unwrap();
        let mut facts = Facts {
            sender_domain: "github.com".into(),
            sender_name: "dependabot[bot]".into(),
            ..Default::default()
        };
        let d = evaluate(std::slice::from_ref(&mesh), &facts, &[], &NoLists);
        assert_eq!(
            d.output.and_then(|o| o.disposition),
            Some(Disposition::Spoil)
        );

        facts.sender_name = "github-actions[bot]".into();
        facts
            .headers
            .insert("x-github-reason".into(), "ci_activity".into());
        let d = evaluate(std::slice::from_ref(&mesh), &facts, &[], &NoLists);
        assert_eq!(
            d.output.and_then(|o| o.disposition),
            Some(Disposition::Filed)
        );

        facts.headers.clear();
        let d = evaluate(&[mesh], &facts, &[], &NoLists);
        assert!(d.output.is_none(), "an absent header matches nothing");
    }

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
    fn a_match_too_close_to_call_lets_a_later_mesh_decide_and_says_so() {
        let promos = mesh(
            "Mesh: Promos\nROUTES\nIF marketing@v1 > .70 → Spoil",
            MeshScope::AllDigs,
            27,
        );
        let newsletters = mesh(
            "Mesh: Newsletters\nROUTES\nIF mailing_list → Filed",
            MeshScope::AllDigs,
            30,
        );
        let policy = ConfidencePolicy::default();
        let list = Facts {
            mailing_list: true,
            ..Default::default()
        };
        let both = [promos.clone(), newsletters.clone()];
        let thin = [ev("marketing", 1, 0.78)];

        // 0.78 clears .70 by 0.08 of the 0.15 Spoil needs: Newsletters decides.
        let d = evaluate_with(&both, &list, &thin, &NoLists, Some(&policy));
        assert_eq!(d.disposition(), Some(&Disposition::Filed));
        assert_eq!(d.mesh_name.as_deref(), Some("Newsletters"));
        assert_eq!(d.passed_over.len(), 1);
        assert!(
            d.passed_over[0].contains("margin 0.08 is below the 0.15 required"),
            "{:?}",
            d.passed_over
        );

        // Nothing after it matches: the thin match is the decision, held as before.
        let d = evaluate_with(&both, &Facts::default(), &thin, &NoLists, Some(&policy));
        assert_eq!(d.disposition(), Some(&Disposition::Spoil));
        assert!(d.passed_over.is_empty());
        assert!(d.margin.unwrap() < 0.15);

        // A clear match stands; without the policy nothing falls through.
        let strong = [ev("marketing", 1, 0.95)];
        let d = evaluate_with(&both, &list, &strong, &NoLists, Some(&policy));
        assert_eq!(d.disposition(), Some(&Disposition::Spoil));
        assert_eq!(
            evaluate(&both, &list, &thin, &NoLists).disposition(),
            Some(&Disposition::Spoil)
        );
    }

    #[test]
    fn a_borderline_find_never_falls_through_to_being_hidden() {
        let policy = ConfidencePolicy::default();
        let asks = mesh(
            "Mesh: Asks\nROUTES\nIF requires_action@v1 > .80 → Find: Do",
            MeshScope::AllDigs,
            10,
        );
        let hide = mesh(
            "Mesh: Lists\nROUTES\nIF mailing_list → Spoil",
            MeshScope::AllDigs,
            20,
        );
        let file = mesh(
            "Mesh: Updates\nROUTES\nIF mailing_list → Filed",
            MeshScope::AllDigs,
            30,
        );
        let list = Facts {
            mailing_list: true,
            ..Default::default()
        };
        // 0.82 clears .80 by 0.02 of the 0.05 a Find needs.
        let thin = [ev("requires_action", 1, 0.82)];

        // Spoil would hide it: skipped, and Filed further on takes it over.
        let d = evaluate_with(
            &[asks.clone(), hide.clone(), file],
            &list,
            &thin,
            &NoLists,
            Some(&policy),
        );
        assert_eq!(d.disposition(), Some(&Disposition::Filed));

        // Only a hiding rule after it: the borderline Find stands, held as Unresolved by Sift.
        let d = evaluate_with(&[asks, hide], &list, &thin, &NoLists, Some(&policy));
        assert_eq!(
            d.disposition(),
            Some(&Disposition::Find {
                find_type: FindType::Do
            })
        );
        assert!(policy
            .hold_reason(DispositionKind::Find, d.margin, "en")
            .is_some());
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
    fn a_named_list_matches_the_sender_and_formats_back_as_written() {
        let m = mesh(
            "Mesh: Keep\nROUTES\nIF sender_domain IN list:keep_promotions → Filed\nELSE → Spoil",
            MeshScope::AllDigs,
            0,
        );
        assert!(crate::format::format(&m).contains("sender_domain IN list:keep_promotions"));
        let named = riddle_core::NamedLists::from_defs([riddle_core::NamedListDef {
            name: "keep_promotions".into(),
            description: String::new(),
            entries: vec!["uber.com".into()],
            pack: None,
        }]);
        let decide = |sender: &str| {
            let facts = Facts {
                sender: sender.into(),
                ..Default::default()
            };
            let lists = crate::simulate::FactsLists(&facts, &named);
            evaluate(std::slice::from_ref(&m), &facts, &[], &lists)
                .disposition()
                .map(|d| d.kind())
        };
        assert_eq!(decide("deals@email.uber.com"), Some(DispositionKind::Filed));
        assert_eq!(decide("deals@lyft.com"), Some(DispositionKind::Spoil));
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
