//! Canonical Mesh text from the AST: what the editor shows after a Director
//! proposal and what `parse` must read back unchanged (round trip).

use riddle_core::{CmpOp, Disposition, Expiry, Expr, FactValue, Mesh, MeshOutput, MeshScope};

fn fmt_threshold(t: f64) -> String {
    if t == 0.0 {
        return "0".into();
    }
    if t == 1.0 {
        return "1".into();
    }
    let s = format!("{t:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    s.strip_prefix('0').map(|r| r.to_string()).unwrap_or(s)
}

fn fmt_value(v: &FactValue) -> String {
    match v {
        FactValue::Bool(b) => b.to_string(),
        FactValue::Number(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        FactValue::Text(t) => {
            if t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
                && !t.is_empty()
                && !t.chars().next().unwrap_or('0').is_ascii_digit()
            {
                t.clone()
            } else {
                format!("\"{t}\"")
            }
        }
    }
}

fn precedence(e: &Expr) -> u8 {
    match e {
        Expr::Or { .. } => 1,
        Expr::And { .. } => 2,
        Expr::Not { .. } => 3,
        _ => 4,
    }
}

fn fmt_child(child: &Expr, parent_prec: u8) -> String {
    let s = fmt_expr(child);
    if precedence(child) < parent_prec {
        format!("({s})")
    } else {
        s
    }
}

pub fn fmt_expr(e: &Expr) -> String {
    match e {
        Expr::Signal {
            signal,
            version,
            choice,
            op,
            threshold,
        } => {
            let c = choice.as_ref().map(|c| format!(":{c}")).unwrap_or_default();
            format!(
                "{signal}@v{version}{c} {} {}",
                op.as_str(),
                fmt_threshold(*threshold)
            )
        }
        Expr::Fact { fact, op, value } => {
            let fact = match fact.strip_prefix(riddle_core::HEADER_PREFIX) {
                Some(h) => format!("header {h}"),
                None => fact.clone(),
            };
            match (op, value) {
                (CmpOp::Eq, FactValue::Bool(true)) => fact,
                (CmpOp::Eq, FactValue::Bool(false)) => format!("NOT {fact}"),
                _ => format!("{fact} {} {}", op.as_str(), fmt_value(value)),
            }
        }
        Expr::InList { subject, list } => format!("{subject} IN {}", list.as_str()),
        Expr::InNamedList { subject, list } => format!("{subject} IN list:{list}"),
        Expr::And { items } => items
            .iter()
            .map(|i| fmt_child(i, 2))
            .collect::<Vec<_>>()
            .join(" AND "),
        Expr::Or { items } => items
            .iter()
            .map(|i| fmt_child(i, 1))
            .collect::<Vec<_>>()
            .join(" OR "),
        Expr::Not { item } => format!("NOT {}", fmt_child(item, 3)),
        Expr::Always => String::new(),
    }
}

pub fn fmt_output(o: &MeshOutput) -> String {
    let mut parts = Vec::new();
    match &o.disposition {
        Some(Disposition::Find { find_type }) => parts.push(format!("Find: {}", find_type.label())),
        Some(Disposition::Filed) => parts.push("Filed".into()),
        Some(Disposition::Spoil) => parts.push("Spoil".into()),
        Some(Disposition::Contamination) => parts.push("Contamination".into()),
        Some(Disposition::Unresolved) | None => {}
    }
    if o.escalate {
        parts.push("Escalate".into());
    }
    for s in &o.strata {
        parts.push(format!("Stratum: {s}"));
    }
    if o.notify {
        parts.push("Notify".into());
    }
    if o.replaces {
        parts.push("Replaces".into());
    }
    if let Some(e) = o.expires {
        parts.push(format!("Expires: {}", fmt_expiry(e)));
    }
    if let Some(i) = o.importance {
        let s = i.as_str();
        parts.push(format!(
            "Importance: {}{}",
            s[..1].to_ascii_uppercase(),
            &s[1..]
        ));
    }
    parts.join(" + ")
}

pub fn fmt_scope(s: &MeshScope) -> String {
    match s {
        MeshScope::AllDigs => "All Digs".into(),
        MeshScope::DigGroup { group } => format!("Group: {group}"),
        MeshScope::Dig { dig_id } => format!("Dig: {dig_id}"),
    }
}

/// The canonical text of a Mesh.
pub fn format(m: &Mesh) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Mesh: {} v{}\n\nScope:\n{}\n",
        m.name,
        m.version,
        fmt_scope(&m.scope)
    ));
    if m.priority != 0 {
        out.push_str(&format!("\nPriority: {}\n", m.priority));
    }
    if !m.routes.is_empty() {
        out.push_str("\nROUTES\n");
        let mut open_block: Option<&Expr> = None;
        for (i, r) in m.routes.iter().enumerate() {
            // Consecutive routes under the same WHEN print as one block.
            if open_block.is_some() && r.when.as_ref() != open_block {
                out.push_str("\nEND\n");
                open_block = None;
            }
            if let (Some(gate), None) = (&r.when, open_block) {
                out.push_str(&format!("\nWHEN {}\n", fmt_expr(gate)));
                open_block = Some(gate);
                let head = match &r.local {
                    Some(c) => format!("IF {}", fmt_expr(c)),
                    None => "ELSE".to_string(),
                };
                out.push_str(&format!(
                    "    {head}\n        → {}\n",
                    fmt_output(&r.output)
                ));
                continue;
            }
            if open_block.is_some() {
                let head = match &r.local {
                    Some(c) => format!("ELSE IF {}", fmt_expr(c)),
                    None => "ELSE".to_string(),
                };
                out.push_str(&format!(
                    "    {head}\n        → {}\n",
                    fmt_output(&r.output)
                ));
                continue;
            }
            let head = match (&r.condition, i) {
                (Expr::Always, _) => "ELSE".to_string(),
                (c, 0) => format!("IF {}", fmt_expr(c)),
                (c, _) => format!("ELSE IF {}", fmt_expr(c)),
            };
            out.push_str(&format!("\n{head}\n    → {}\n", fmt_output(&r.output)));
        }
        if open_block.is_some() {
            out.push_str("\nEND\n");
        }
    }
    if !m.guards.is_empty() {
        out.push_str("\nGUARDS\n\n");
        for g in &m.guards {
            let kind = match g.never {
                riddle_core::DispositionKind::Spoil => "Spoil",
                riddle_core::DispositionKind::Contamination => "Contamination",
                riddle_core::DispositionKind::Filed => "Filed",
                riddle_core::DispositionKind::Find => "Find",
                riddle_core::DispositionKind::Unresolved => "Unresolved",
            };
            out.push_str(&format!("NEVER {kind} IF {}\n", fmt_expr(&g.condition)));
        }
    }
    out
}

/// `48` → `(2, "day")`, `18` → `(18, "hour")`: whole days when it divides,
/// so `Expires:` and its sentence agree on the unit.
pub fn split_hours(h: u32) -> (u32, &'static str) {
    if h.is_multiple_of(24) {
        (h / 24, "day")
    } else {
        (h, "hour")
    }
}

/// What `Expires:` says, in the form the parser reads back.
pub fn fmt_expiry(e: Expiry) -> String {
    match e {
        Expiry::After { hours } => fmt_hours(hours),
        Expiry::AfterMinutes { minutes } => format!("{minutes}m"),
        Expiry::EndOfDay => "end of day".into(),
        Expiry::NextMorning => "next morning".into(),
        Expiry::AfterDates => "after its dates".into(),
    }
}

/// `48` → `2d`, `18` → `18h`: the shortest form `Expires:` reads back.
pub fn fmt_hours(h: u32) -> String {
    let (n, unit) = split_hours(h);
    format!("{n}{}", &unit[..1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    const DOC_EXAMPLE: &str = "Mesh: GitHub Notifications v8\n\nScope:\nWork Digs\n\nROUTES\n\nIF review_requested@v3 > .80\n    → Find: Review\n\nELSE IF ci_failure@v4 > .85\n     AND personally_relevant@v3 > .70\n    → Find: Do\n\nELSE IF routine@v1 > .90\n     AND personally_relevant@v3 < .30\n    → Spoil\n\nELSE\n    → Escalate\n\nGUARDS\n\nNEVER Spoil IF sender IN never_hidden\n";

    #[test]
    fn the_doc_example_round_trips_through_format_and_parse() {
        let a = parse(DOC_EXAMPLE).unwrap();
        let text = format(&a);
        let b = parse(&text).unwrap();
        assert_eq!(a.name, b.name);
        assert_eq!(a.version, b.version);
        assert_eq!(a.scope, b.scope);
        assert_eq!(
            a.routes
                .iter()
                .map(|r| (&r.condition, &r.output))
                .collect::<Vec<_>>(),
            b.routes
                .iter()
                .map(|r| (&r.condition, &r.output))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            a.guards
                .iter()
                .map(|g| (&g.never, &g.condition))
                .collect::<Vec<_>>(),
            b.guards
                .iter()
                .map(|g| (&g.never, &g.condition))
                .collect::<Vec<_>>()
        );
        assert!(text.contains("review_requested@v3 > .8\n"));
        assert!(text.contains("NEVER Spoil IF sender IN never_hidden"));
    }

    #[test]
    fn nested_boolean_expressions_keep_their_parentheses() {
        let m = parse("ROUTES\nIF (a@v1 > .5 OR b@v1 > .5) AND NOT (mailing_list OR precedence_bulk) → Spoil + Stratum: Needs Response").unwrap();
        let text = format(&m);
        assert!(
            text.contains("(a@v1 > .5 OR b@v1 > .5) AND NOT (mailing_list OR precedence_bulk)"),
            "{text}"
        );
        assert!(text.contains("Stratum: Needs Response"));
        let again = parse(&text).unwrap();
        assert_eq!(again.routes[0].condition, m.routes[0].condition);
        assert_eq!(again.routes[0].output, m.routes[0].output);
    }
}
