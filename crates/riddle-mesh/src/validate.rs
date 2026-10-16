//! Mesh validation: referenced facts must exist in `Facts::NAMES`, Signal
//! references must resolve in the registry, list subjects must be `sender`.
//! The resolver is a trait so tests validate without a store.

use riddle_core::{Expr, Facts, Mesh, Signal, SignalStatus};
use riddle_errors::{Result, RiddleError};
use riddle_store::{repo, Store};
use std::sync::Arc;

pub trait SignalResolver: Send + Sync {
    /// The Signal version, if it exists and may be referenced.
    fn resolve(&self, name: &str, version: u32) -> Option<Signal>;
}

/// Resolves against the registry in the index store; Rejected versions are
/// not referenceable.
pub struct RegistryResolver {
    pub index: Arc<Store>,
}

impl SignalResolver for RegistryResolver {
    fn resolve(&self, name: &str, version: u32) -> Option<Signal> {
        repo::signals::get(&self.index, name, version)
            .ok()
            .flatten()
            .filter(|s| s.status != SignalStatus::Rejected)
    }
}

/// A resolver over a fixed list, for tests and the parse preview.
pub struct StaticResolver(pub Vec<Signal>);

impl SignalResolver for StaticResolver {
    fn resolve(&self, name: &str, version: u32) -> Option<Signal> {
        self.0
            .iter()
            .find(|s| s.name == name && s.version == version)
            .cloned()
    }
}

/// Accepts every reference; for parsing Meshes whose registry is elsewhere.
pub struct PermissiveResolver;

impl SignalResolver for PermissiveResolver {
    fn resolve(&self, name: &str, version: u32) -> Option<Signal> {
        let mut s = Signal::noul(name, "permissive");
        s.version = version;
        Some(s)
    }
}

fn check_expr(e: &Expr, resolver: &dyn SignalResolver, problems: &mut Vec<String>) {
    match e {
        Expr::Signal {
            signal,
            version,
            choice,
            ..
        } => match resolver.resolve(signal, *version) {
            None => problems.push(format!("signal {signal}@v{version} is not in the registry")),
            Some(s) => {
                if let Some(c) = choice {
                    if !s.choices.iter().any(|x| x.name.eq_ignore_ascii_case(c)) {
                        problems.push(format!("signal {signal}@v{version} has no choice {c}"));
                    }
                }
            }
        },
        Expr::Fact { fact, .. } => {
            if !Facts::NAMES.contains(&fact.as_str())
                && !fact.starts_with(riddle_core::HEADER_PREFIX)
            {
                problems.push(format!("fact {fact} does not exist"));
            }
        }
        Expr::InList { subject, .. } => {
            if subject != "sender" {
                problems.push(format!("IN applies to sender, not {subject}"));
            }
        }
        Expr::InNamedList { subject, list } => {
            let text_fact = matches!(
                Facts::default().get(subject),
                Some(riddle_core::FactValue::Text(_))
            );
            if subject != "sender" && subject != "sender_domain" && !text_fact {
                problems.push(format!(
                    "IN list:{list} applies to the sender or a text fact such as sender_name, not {subject}"
                ));
            }
            if !riddle_core::NamedListDef::valid_name(list) {
                problems.push(format!(
                    "list:{list} is not a list name (lowercase letters, digits and _)"
                ));
            }
        }
        Expr::And { items } | Expr::Or { items } => {
            items.iter().for_each(|i| check_expr(i, resolver, problems))
        }
        Expr::Not { item } => check_expr(item, resolver, problems),
        Expr::Always => {}
    }
}

pub fn problems(m: &Mesh, resolver: &dyn SignalResolver) -> Vec<String> {
    let mut problems = Vec::new();
    if m.name.trim().is_empty() {
        problems.push("the Mesh has no name".into());
    }
    for r in &m.routes {
        check_expr(&r.condition, resolver, &mut problems);
    }
    for g in &m.guards {
        check_expr(&g.condition, resolver, &mut problems);
    }
    problems.sort();
    problems.dedup();
    problems
}

pub fn validate(m: &Mesh, resolver: &dyn SignalResolver) -> Result<()> {
    let p = problems(m, resolver);
    if p.is_empty() {
        Ok(())
    } else {
        Err(
            RiddleError::invalid(format!("Mesh {} is invalid: {}", m.name, p.join("; ")))
                .with_detail(serde_json::json!({ "problems": p })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    #[test]
    fn unknown_facts_and_unregistered_signals_are_reported_together() {
        let m = parse("Mesh: T\nROUTES\nIF review_requested@v9 > .8 AND no_such_fact → Spoil\nGUARDS\nNEVER Spoil IF sender IN vips").unwrap();
        let mut ok = Signal::noul("review_requested", "x");
        ok.version = 3;
        let err = validate(&m, &StaticResolver(vec![ok])).unwrap_err();
        let problems = err.detail.unwrap()["problems"].as_array().unwrap().len();
        assert_eq!(problems, 2);
        let fine =
            parse("Mesh: T\nROUTES\nIF review_requested@v3 > .8 AND mailing_list → Spoil").unwrap();
        let mut ok = Signal::noul("review_requested", "x");
        ok.version = 3;
        validate(&fine, &StaticResolver(vec![ok])).unwrap();
    }
}
