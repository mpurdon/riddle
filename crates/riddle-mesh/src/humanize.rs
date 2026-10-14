//! A Mesh rule in words. The Why card used to lead with the matched route as
//! written (`Payments v1: ELSE IF payment_related@v1 > .80 AND
//! requires_action@v1 > .70 → Find: Pay + Stratum: Finance`), which is the
//! grammar the evaluator reads and not a sentence a person would say. This
//! module says it: "It appears payment related is very likely and requires
//! action is likely, so it is something to pay under Finance." It is
//! deterministic and free: a rule is read from its AST, a decision from the
//! same AST plus the facts and scores the decision used, and no model is
//! asked to rephrase anything.
//!
//! The words for a probability are calibrated language, not invention. The
//! vocabulary is Sherman Kent's "Words of Estimative Probability" (1964) as
//! standardised by Intelligence Community Directive 203 and the IPCC's
//! likelihood scale; the bands are Riddle's, cut where Mesh thresholds sit:
//!
//! | score       | words                   |
//! |-------------|-------------------------|
//! | ≥ .99       | virtually certain       |
//! | ≥ .95       | almost certain          |
//! | ≥ .90       | extremely likely        |
//! | ≥ .80       | very likely             |
//! | ≥ .65       | likely                  |
//! | > .50       | more likely than not    |
//! | .45 – .50   | about as likely as not  |
//! | .20 – .45   | unlikely                |
//! | .05 – .20   | very unlikely           |
//! | < .05       | almost certainly not    |
//!
//! A rule reads its thresholds ("> .80" is "very likely"); a decision reads
//! the message's actual score, so a route that fired on `> .80` with a score
//! of .96 says "almost certain". The two never contradict: a score that
//! clears a threshold always reads at least as strong as the threshold.

use riddle_core::{
    CmpOp, Disposition, DispositionKind, Expiry, Expr, FactValue, Facts, FindType, Guard,
    MeshOutput, NamedList, Route, SignalEvaluation, HEADER_PREFIX, NEXT_MORNING_HOUR,
};

/// Calibrated words for a probability. See the module header for the bands.
pub fn likelihood(p: f64) -> &'static str {
    match p {
        p if p >= 0.99 => "virtually certain",
        p if p >= 0.95 => "almost certain",
        p if p >= 0.90 => "extremely likely",
        p if p >= 0.80 => "very likely",
        p if p >= 0.65 => "likely",
        p if p > 0.50 => "more likely than not",
        p if p >= 0.45 => "about as likely as not",
        p if p >= 0.20 => "unlikely",
        p if p >= 0.05 => "very unlikely",
        _ => "almost certainly not",
    }
}

/// What a decision is read against: the facts and Signal scores it used.
/// Without it, a rule is read as written.
pub struct Evidence<'a> {
    pub facts: &'a Facts,
    pub evaluations: &'a [SignalEvaluation],
}

/// Fill the readings of a Mesh parsed or stored before they existed.
pub fn fill(mesh: &mut riddle_core::Mesh) {
    for r in &mut mesh.routes {
        if r.words.is_empty() {
            r.words = route_reading(r);
        }
    }
    for g in &mut mesh.guards {
        if g.words.is_empty() {
            g.words = guard_reading(g);
        }
    }
}

/// A route as a rule: "When the sender's domain is github.com and review
/// requested is very likely, it is something to review under Engineering."
pub fn route_reading(r: &Route) -> String {
    route_words(r, None)
}

/// A route as the reason for one decision: "It appears the sender's domain
/// is github.com and review requested is almost certain, so it is something
/// to review under Engineering."
pub fn route_decision(r: &Route, evidence: &Evidence) -> String {
    route_words(r, Some(evidence))
}

/// The one sentence behind both readings. With evidence it is the reason
/// for a decision, in the past tense; without, the rule as written.
fn route_words(r: &Route, evidence: Option<&Evidence>) -> String {
    let decided = evidence.is_some();
    let condition = match (&r.when, &r.local) {
        (Some(when), Some(local)) => block_words(when, local, evidence),
        (Some(when), None) => format!(
            "{}{}",
            expr_words(when, evidence),
            if decided {
                ", and nothing more specific applied"
            } else {
                " and nothing more specific applies"
            }
        ),
        (None, _) => expr_words(&r.condition, evidence),
    };
    let output = output_words(&r.output);
    sentence(&match (condition.is_empty(), decided) {
        (true, true) => format!("nothing above applied, so {output}"),
        (true, false) => format!("otherwise, {output}"),
        (false, true) => format!("it appears {condition}, so {output}"),
        (false, false) => format!("when {condition}, {output}"),
    })
}

/// A block's condition and the route's own, as one list of clauses.
fn block_words(when: &Expr, local: &Expr, evidence: Option<&Evidence>) -> String {
    let mut parts = vec![expr_words(when, evidence)];
    match local {
        Expr::And { items } => parts.extend(items.iter().map(|i| expr_words(i, evidence))),
        other => parts.push(expr_words(other, evidence)),
    }
    join(&parts, "and")
}

/// A guard: "Never Spoil when the sender is on your Never Hidden list."
pub fn guard_reading(g: &Guard) -> String {
    let never = match g.never {
        DispositionKind::Find => "a Find",
        DispositionKind::Filed => "Filed",
        DispositionKind::Spoil => "Spoil",
        DispositionKind::Contamination => "Contamination",
        DispositionKind::Unresolved => "Unresolved",
    };
    sentence(&format!(
        "never {never} when {}",
        expr_words(&g.condition, None)
    ))
}

// "Next morning" reads as "9 am"; a later hour would need other words.
const _: () = assert!(NEXT_MORNING_HOUR < 12);

/// What a route decides, as a clause: "it is something to pay under Finance".
pub fn output_words(o: &MeshOutput) -> String {
    let mut s = match &o.disposition {
        Some(Disposition::Find { find_type }) => {
            format!("it is something to {}", find_verb(*find_type))
        }
        Some(Disposition::Filed) => "it is filed".to_string(),
        Some(Disposition::Spoil) => "it is set aside as Spoil".to_string(),
        Some(Disposition::Contamination) => "it is set apart as Contamination".to_string(),
        Some(Disposition::Unresolved) => "it is left for you to decide".to_string(),
        None if o.escalate => "the judge decides".to_string(),
        None => "nothing is decided".to_string(),
    };
    if !o.strata.is_empty() {
        s.push_str(" under ");
        s.push_str(&join(&o.strata, "and"));
    }
    if o.escalate && o.disposition.is_some() {
        s.push_str(", and the judge is asked");
    }
    if let Some(i) = o.importance {
        s.push_str(&format!(", of {} importance", i.as_str()));
    }
    if o.notify {
        s.push_str(", and you are told at once");
    }
    if o.replaces {
        s.push_str("; a newer one from the same sender replaces it");
    }
    match o.expires {
        Some(Expiry::After { hours }) => {
            let span = match crate::format::split_hours(hours) {
                (1, unit) => format!("a{} {unit}", if unit == "hour" { "n" } else { "" }),
                (n, unit) => riddle_core::text::plural(n as usize, unit),
            };
            s.push_str(&format!("; it expires {span} after it arrives"));
        }
        Some(Expiry::AfterMinutes { minutes }) => s.push_str(&format!(
            "; it expires {} after it arrives",
            riddle_core::text::plural(minutes as usize, "minute")
        )),
        Some(Expiry::EndOfDay) => s.push_str("; it expires at the end of the day it arrives"),
        Some(Expiry::NextMorning) => s.push_str(&format!(
            "; it expires at {NEXT_MORNING_HOUR} am the next morning"
        )),
        None => {}
    }
    s
}

/// A condition as a clause, lowercase and without a full stop. With
/// evidence it describes the message ("review requested is almost certain",
/// "the thread has 4 messages"); without, the rule ("review requested is
/// very likely", "the thread has more than 3 messages").
pub fn expr_words(e: &Expr, evidence: Option<&Evidence>) -> String {
    match e {
        Expr::Signal {
            signal,
            version,
            choice,
            op,
            threshold,
        } => signal_words(
            signal,
            *version,
            choice.as_deref(),
            *op,
            *threshold,
            evidence,
        ),
        Expr::Fact { fact, op, value } => fact_words(fact, *op, value, evidence),
        Expr::InList { subject, list } => {
            format!("{} is {}", subject_words(subject), list_words(*list))
        }
        Expr::InNamedList { list, .. } => format!("the sender is on your list {list:?}"),
        Expr::And { items } => join(
            &distinct(items.iter().map(|i| expr_words(i, evidence)).collect()),
            "and",
        ),
        Expr::Or { items } => {
            let parts = distinct(items.iter().map(|i| expr_words(i, evidence)).collect());
            if parts.len() == 2 {
                format!("either {} or {}", parts[0], parts[1])
            } else {
                join(&parts, "or")
            }
        }
        Expr::Not { item } => not_words(item, evidence),
        Expr::Always => String::new(),
    }
}

fn not_words(inner: &Expr, evidence: Option<&Evidence>) -> String {
    match inner {
        // The message's own value already says it; a negation would repeat it.
        Expr::Signal { .. } | Expr::Fact { .. } if evidence.is_some() => {
            expr_words(inner, evidence)
        }
        Expr::Signal {
            signal,
            version,
            choice,
            op,
            threshold,
        } => {
            let words = signal_words(signal, *version, choice.as_deref(), *op, *threshold, None);
            words.replacen(" is ", " is not ", 1)
        }
        Expr::Fact { fact, op, value } => {
            let flipped = match op {
                CmpOp::Eq => Some(CmpOp::Neq),
                CmpOp::Neq => Some(CmpOp::Eq),
                CmpOp::Gt => Some(CmpOp::Lte),
                CmpOp::Gte => Some(CmpOp::Lt),
                CmpOp::Lt => Some(CmpOp::Gte),
                CmpOp::Lte => Some(CmpOp::Gt),
                CmpOp::Contains => None,
            };
            match flipped {
                Some(op) => fact_words(fact, op, value, None),
                None => format!("{} does not contain {}", subject_words(fact), quoted(value)),
            }
        }
        Expr::InList { subject, list } => {
            format!("{} is not {}", subject_words(subject), list_words(*list))
        }
        Expr::InNamedList { list, .. } => format!("the sender is not on your list {list:?}"),
        Expr::And { items } => {
            let parts: Vec<String> = items.iter().map(|i| expr_words(i, evidence)).collect();
            if parts.len() == 2 {
                format!("not both {} and {}", parts[0], parts[1])
            } else {
                format!("not all of {}", join(&parts, "and"))
            }
        }
        Expr::Or { items } => {
            let parts: Vec<String> = items.iter().map(|i| expr_words(i, evidence)).collect();
            if parts.len() == 2 {
                format!("neither {} nor {}", parts[0], parts[1])
            } else {
                format!("none of {}", join(&parts, "or"))
            }
        }
        Expr::Not { item } => expr_words(item, evidence),
        Expr::Always => String::new(),
    }
}

fn signal_words(
    signal: &str,
    version: u32,
    choice: Option<&str>,
    op: CmpOp,
    threshold: f64,
    evidence: Option<&Evidence>,
) -> String {
    let name = name_words(signal);
    let choice_tail = choice
        .map(|c| format!(" {}", c.to_ascii_lowercase()))
        .unwrap_or_default();
    if let Some(ev) = evidence {
        return match ev
            .evaluations
            .iter()
            .find(|x| x.signal_name == signal && x.signal_version == version)
        {
            Some(x) => format!("{name} is {}{choice_tail}", likelihood(x.score(choice))),
            None => format!("{name} was not scored"),
        };
    }
    // A rule names a floor or a ceiling; the band just past the threshold
    // is the one every qualifying score falls in.
    const NUDGE: f64 = 1e-9;
    let p = match op {
        CmpOp::Neq => return format!("{name} is not {}{choice_tail}", likelihood(threshold)),
        CmpOp::Contains => return format!("{name} contains {threshold}"),
        CmpOp::Gt => threshold + NUDGE,
        CmpOp::Lt => threshold - NUDGE,
        CmpOp::Gte | CmpOp::Eq | CmpOp::Lte => threshold,
    };
    format!("{name} is {}{choice_tail}", likelihood(p))
}

fn fact_words(fact: &str, op: CmpOp, value: &FactValue, evidence: Option<&Evidence>) -> String {
    let actual = evidence.and_then(|ev| ev.facts.get(fact));
    match (value, actual) {
        (FactValue::Bool(_), Some(FactValue::Bool(actual))) => bool_words(fact, actual),
        (FactValue::Bool(b), None) => {
            let holds = match op {
                CmpOp::Neq => !*b,
                _ => *b,
            };
            bool_words(fact, holds)
        }
        (FactValue::Number(_), Some(FactValue::Number(actual))) => number_actual(fact, actual),
        (FactValue::Number(n), _) => number_rule(fact, op, *n),
        (FactValue::Text(t), actual) => text_words(fact, op, t, actual),
        (FactValue::Bool(b), Some(_)) => bool_words(fact, *b),
    }
}

fn text_words(fact: &str, op: CmpOp, value: &str, actual: Option<FactValue>) -> String {
    let subject = subject_words(fact);
    let actual = actual.and_then(|v| match v {
        FactValue::Text(t) => Some(t),
        _ => None,
    });
    if fact == "sender_cadence" {
        return cadence_words(op, value, actual.as_deref());
    }
    match (op, actual) {
        (CmpOp::Contains, Some(actual)) => {
            if actual.to_lowercase().contains(&value.to_lowercase()) {
                format!("{subject} contains {}", quote(value))
            } else {
                format!("{subject} does not contain {}", quote(value))
            }
        }
        (CmpOp::Contains, None) => format!("{subject} contains {}", quote(value)),
        (_, Some(actual)) if actual.is_empty() => format!("{subject} is empty"),
        (_, Some(actual)) => format!("{subject} is {actual}"),
        (CmpOp::Neq, None) => format!("{subject} is not {value}"),
        (_, None) => format!("{subject} is {value}"),
    }
}

/// `sender_cadence = daily` → "the sender writes daily".
fn cadence_words(op: CmpOp, value: &str, actual: Option<&str>) -> String {
    let rhythm = |c: &str| match c {
        "weekdays" => "on weekdays".to_string(),
        "irregular" => "irregularly".to_string(),
        "every_few_days" => "every few days".to_string(),
        c => c.to_string(),
    };
    match (op, actual) {
        (_, Some("")) => "the sender has not written often enough to say how often".into(),
        (_, Some(actual)) => format!("the sender writes {}", rhythm(actual)),
        (CmpOp::Neq, None) => format!("the sender does not write {}", rhythm(value)),
        (_, None) => format!("the sender writes {}", rhythm(value)),
    }
}

fn number_rule(fact: &str, op: CmpOp, n: f64) -> String {
    let n = num(n);
    match fact {
        "thread_length" => match op {
            CmpOp::Gt => format!("the thread has more than {n} messages"),
            CmpOp::Gte => format!("the thread has at least {n} messages"),
            CmpOp::Lt => format!("the thread has fewer than {n} messages"),
            CmpOp::Lte => format!("the thread has at most {n} messages"),
            CmpOp::Neq => format!("the thread does not have {n} messages"),
            CmpOp::Eq | CmpOp::Contains => format!("the thread has {n} messages"),
        },
        "deadline_days" => match op {
            CmpOp::Gt => format!("the deadline is more than {n} days away"),
            CmpOp::Gte => format!("the deadline is at least {n} days away"),
            CmpOp::Lt => format!("the deadline is less than {n} days away"),
            CmpOp::Lte => format!("the deadline is within {n} days"),
            CmpOp::Neq => format!("the deadline is not {n} days away"),
            CmpOp::Eq | CmpOp::Contains => format!("the deadline is {n} days away"),
        },
        _ => {
            let subject = subject_words(fact);
            match op {
                CmpOp::Gt => format!("{subject} is more than {n}"),
                CmpOp::Gte => format!("{subject} is at least {n}"),
                CmpOp::Lt => format!("{subject} is less than {n}"),
                CmpOp::Lte => format!("{subject} is at most {n}"),
                CmpOp::Neq => format!("{subject} is not {n}"),
                CmpOp::Eq | CmpOp::Contains => format!("{subject} is {n}"),
            }
        }
    }
}

fn number_actual(fact: &str, actual: f64) -> String {
    match fact {
        "thread_length" => match actual as i64 {
            1 => "the thread has one message".to_string(),
            n => format!("the thread has {n} messages"),
        },
        "deadline_days" if actual >= i64::MAX as f64 => "there is no deadline".to_string(),
        "deadline_days" => match actual as i64 {
            0 => "the deadline is today".to_string(),
            1 => "the deadline is tomorrow".to_string(),
            n if n < 0 => format!("the deadline was {} days ago", -n),
            n => format!("the deadline is {n} days away"),
        },
        _ => format!("{} is {}", subject_words(fact), num(actual)),
    }
}

/// A boolean fact as a statement about the message, both ways round.
fn bool_words(fact: &str, holds: bool) -> String {
    let (yes, no) = match fact {
        "sender_in_contacts" => (
            "the sender is in your contacts",
            "the sender is not in your contacts",
        ),
        "sender_replied_to_before" => (
            "you have replied to the sender before",
            "you have never replied to the sender",
        ),
        "first_time_sender" => (
            "the sender is writing for the first time",
            "the sender has written before",
        ),
        "same_organisation" => (
            "the sender is in your organisation",
            "the sender is outside your organisation",
        ),
        "mailing_list" => (
            "it came through a mailing list",
            "it did not come through a mailing list",
        ),
        "list_unsubscribe" => (
            "it carries an unsubscribe link",
            "it carries no unsubscribe link",
        ),
        "precedence_bulk" => ("it was sent as bulk mail", "it was not sent as bulk mail"),
        "auto_submitted" => ("it was sent by a machine", "it was not sent by a machine"),
        "spf_pass" => ("SPF passed", "SPF did not pass"),
        "dkim_pass" => ("DKIM passed", "DKIM did not pass"),
        "dmarc_pass" => ("DMARC passed", "DMARC did not pass"),
        "reply_to_mismatch" => (
            "the reply-to address differs from the sender",
            "the reply-to address matches the sender",
        ),
        "gmail_important" => (
            "Gmail marked it important",
            "Gmail did not mark it important",
        ),
        "gmail_spam" => ("Gmail marked it spam", "Gmail did not mark it spam"),
        "directly_addressed" => (
            "it was addressed to you directly",
            "it was not addressed to you directly",
        ),
        "cc_only" => ("you were only copied", "you were not merely copied"),
        "has_calendar_part" => (
            "it carries a calendar invitation",
            "it carries no calendar invitation",
        ),
        "has_structured_receipt" => (
            "it carries a structured receipt",
            "it carries no structured receipt",
        ),
        "has_attachments" => ("it has attachments", "it has no attachments"),
        "urgency_language_present" => ("it uses urgent language", "it uses no urgent language"),
        "asks_for_payment_or_credentials" => (
            "it asks for payment or credentials",
            "it does not ask for payment or credentials",
        ),
        "has_deadline" => ("it mentions a deadline", "it mentions no deadline"),
        "is_sent" => ("you sent it", "you did not send it"),
        "lookalike_domain" => (
            "the sender's domain imitates one you know",
            "the sender's domain imitates nothing you know",
        ),
        "sender_in_never_hidden" => (
            "the sender is on your Never Hidden list",
            "the sender is not on your Never Hidden list",
        ),
        "sender_is_vip" => ("the sender is a VIP", "the sender is not a VIP"),
        "automated_sender" => ("it was sent automatically", "it was not sent automatically"),
        "known_sender" => ("you know the sender", "you do not know the sender"),
        "noreply_sender" => (
            "the sender's address takes no replies",
            "the sender's address takes replies",
        ),
        "known_thread" => (
            "you or someone you know already wrote in this thread",
            "nobody you know has written in this thread",
        ),
        "known_domain" => (
            "you know someone at the sender's organisation",
            "you know nobody at the sender's organisation",
        ),
        "government_sender" => (
            "it is from an official government sender",
            "it is not from an official government sender",
        ),
        "from_me" => ("you wrote it", "you did not write it"),
        "free_mail_sender" => (
            "the sender uses a free mailbox such as Gmail",
            "the sender does not use a free mailbox",
        ),
        "glyph_substitution" => (
            "its letters are disguised to get past filters",
            "its letters are not disguised",
        ),
        "suspicious_domain" => (
            "the sender's domain looks throwaway",
            "the sender's domain looks ordinary",
        ),
        "cold_outreach" => (
            "it is unsolicited mail from a stranger's free mailbox",
            "it is not unsolicited mail from a stranger",
        ),
        "from_community" => (
            "it comes from a community you belong to",
            "it does not come from a community",
        ),
        "payment_failed" => ("a payment did not go through", "no payment failed"),
        "payment_recovered" => (
            "a payment went through on a backup method",
            "no payment fell back to a backup method",
        ),
        "has_verification_code" => (
            "it carries a one-time sign-in code",
            "it carries no sign-in code",
        ),
        "sender_on_schedule" => (
            "the sender writes at the same time each day",
            "the sender does not write at a set time",
        ),
        _ => {
            return if holds {
                name_words(fact)
            } else {
                format!("not {}", name_words(fact))
            }
        }
    };
    (if holds { yes } else { no }).to_string()
}

/// The thing a text or number fact is about: "the sender's domain".
fn subject_words(fact: &str) -> String {
    if let Some(h) = fact.strip_prefix(HEADER_PREFIX) {
        return format!("the {h} header");
    }
    match fact {
        "sender_domain" => "the sender's domain".into(),
        "sender" => "the sender".into(),
        "sender_name" => "the sender's name".into(),
        "subject" => "the subject".into(),
        "gmail_category" => "the Gmail category".into(),
        "github_reason" => "the GitHub notification reason".into(),
        "github_event" => "what happened on GitHub".into(),
        "list_id" => "the mailing list".into(),
        "verification_code" => "the sign-in code".into(),
        "community_platform" => "the community".into(),
        "language" => "the language".into(),
        "thread_length" => "the thread length".into(),
        "deadline_days" => "the deadline".into(),
        _ => format!("the {}", name_words(fact)),
    }
}

fn list_words(list: NamedList) -> &'static str {
    match list {
        NamedList::NeverHidden => "on your Never Hidden list",
        NamedList::Contacts => "in your contacts",
        NamedList::Vips => "a VIP",
        NamedList::Organisation => "in your organisation",
        NamedList::Family => "family",
    }
}

fn find_verb(t: FindType) -> &'static str {
    match t {
        FindType::Respond => "respond to",
        FindType::Decide => "decide",
        FindType::Review => "review",
        FindType::Do => "do",
        FindType::Pay => "pay",
        FindType::Attend => "attend",
        FindType::FollowUp => "follow up",
        FindType::Know => "know",
    }
}

/// `deployment_failed` → "deployment failed"; initialisms stay upper-case.
fn name_words(name: &str) -> String {
    name.split('_')
        .map(|w| match w {
            "ci" | "cc" | "spf" | "dkim" | "dmarc" | "vip" | "url" | "otp" | "pr" => {
                w.to_ascii_uppercase()
            }
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each phrase once, in order. With the evidence filled in, every branch of
/// `sender_cadence = daily OR … = every_few_days OR … = weekly` reads as the
/// sender's actual rhythm, and the reason said "the sender writes daily, the
/// sender writes daily or the sender writes daily".
fn distinct(parts: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in parts {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn join(parts: &[String], last: &str) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} {last} {b}"),
        [init @ .., tail] => format!("{} {last} {tail}", init.join(", ")),
    }
}

fn num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

fn quoted(v: &FactValue) -> String {
    match v {
        FactValue::Text(t) => quote(t),
        FactValue::Number(n) => num(*n),
        FactValue::Bool(b) => b.to_string(),
    }
}

fn sentence(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use riddle_core::{Distribution, MessageId};

    fn ev(name: &str, score: f64) -> SignalEvaluation {
        let mut distribution = Distribution::default();
        distribution.insert("YES".into(), score);
        distribution.insert("NO".into(), 1.0 - score);
        SignalEvaluation {
            message_id: MessageId::from("m1"),
            signal_name: name.into(),
            signal_version: 1,
            engine_identity: "null".into(),
            selected: if score >= 0.5 { "YES" } else { "NO" }.into(),
            distribution: distribution.clone(),
            calibrated: distribution,
            confidence: score,
            latency_ms: 0,
            input_tokens: 0,
            cached: false,
            deterministic: false,
            context_hash: None,
            request_hash: None,
            evaluated_at: chrono::Utc::now(),
        }
    }

    fn route(text: &str) -> Route {
        parse(&format!("Mesh: T\nROUTES\n{text}"))
            .unwrap()
            .routes
            .remove(0)
    }

    #[test]
    fn an_or_whose_branches_read_the_same_says_it_once() {
        let m =
            parse("Mesh: T\nROUTES\nIF sender_cadence = daily OR sender_cadence = weekly → Spoil")
                .unwrap();
        let facts = riddle_core::Facts {
            sender_cadence: "daily".into(),
            ..Default::default()
        };
        let words = route_decision(
            &m.routes[0],
            &Evidence {
                facts: &facts,
                evaluations: &[],
            },
        );
        assert_eq!(
            words.matches("the sender writes daily").count(),
            1,
            "{words}"
        );
    }

    #[test]
    fn the_likelihood_ladder_uses_calibrated_words_at_the_thresholds_meshes_use() {
        let cases = [
            (1.0, "virtually certain"),
            (0.99, "virtually certain"),
            (0.95, "almost certain"),
            (0.9, "extremely likely"),
            (0.85, "very likely"),
            (0.8, "very likely"),
            (0.7, "likely"),
            (0.65, "likely"),
            (0.6, "more likely than not"),
            (0.51, "more likely than not"),
            (0.5, "about as likely as not"),
            (0.45, "about as likely as not"),
            (0.3, "unlikely"),
            (0.2, "unlikely"),
            (0.1, "very unlikely"),
            (0.05, "very unlikely"),
            (0.01, "almost certainly not"),
            (0.0, "almost certainly not"),
        ];
        for (p, words) in cases {
            assert_eq!(likelihood(p), words, "{p}");
        }
    }

    #[test]
    fn the_github_deployment_route_reads_as_a_rule() {
        let r = route("IF sender_domain = github.com AND deployment_failed@v1 > .85 AND owned_work_failed@v1 > .60 → Find: Do + Stratum: Engineering");
        assert_eq!(
            route_reading(&r),
            "When the sender's domain is github.com, deployment failed is very likely and owned work failed is more likely than not, it is something to do under Engineering."
        );
    }

    #[test]
    fn a_decision_reads_the_scores_the_message_actually_had() {
        let r = route("IF sender_domain = github.com AND deployment_failed@v1 > .85 AND owned_work_failed@v1 > .60 → Find: Do + Stratum: Engineering");
        let facts = Facts {
            sender_domain: "github.com".into(),
            ..Default::default()
        };
        let evaluations = [ev("deployment_failed", 0.96), ev("owned_work_failed", 0.71)];
        let evidence = Evidence {
            facts: &facts,
            evaluations: &evaluations,
        };
        assert_eq!(
            route_decision(&r, &evidence),
            "It appears the sender's domain is github.com, deployment failed is almost certain and owned work failed is likely, so it is something to do under Engineering."
        );
    }

    #[test]
    fn the_security_and_payment_examples_read_as_sentences() {
        let security = route("IF security_alert@v1 > .80 → Find: Know + Stratum: Notifications");
        assert_eq!(
            route_reading(&security),
            "When security alert is very likely, it is something to know under Notifications."
        );
        let pay = route("IF payment_related@v1 > .80 AND requires_action@v1 > .70 → Find: Pay + Stratum: Finance");
        assert_eq!(
            route_reading(&pay),
            "When payment related is very likely and requires action is likely, it is something to pay under Finance."
        );
    }

    #[test]
    fn a_signal_the_judge_did_not_score_says_so() {
        let r = route("IF review_requested@v1 > .80 → Find: Review");
        let facts = Facts::default();
        let evidence = Evidence {
            facts: &facts,
            evaluations: &[],
        };
        assert_eq!(
            route_decision(&r, &evidence),
            "It appears review requested was not scored, so it is something to review."
        );
    }

    #[test]
    fn a_block_route_and_an_else_read_with_their_block() {
        let m = parse("Mesh: T\nROUTES\nWHEN sender_domain = github.com\n    IF sender_name CONTAINS \"dependabot\" → Spoil\n    ELSE → Filed + Stratum: Engineering\nEND\nELSE → Escalate").unwrap();
        assert_eq!(
            route_reading(&m.routes[0]),
            "When the sender's domain is github.com and the sender's name contains \"dependabot\", it is set aside as Spoil."
        );
        assert_eq!(
            route_reading(&m.routes[1]),
            "When the sender's domain is github.com and nothing more specific applies, it is filed under Engineering."
        );
        assert_eq!(route_reading(&m.routes[2]), "Otherwise, the judge decides.");
        let facts = Facts {
            sender_domain: "github.com".into(),
            sender_name: "dependabot[bot]".into(),
            ..Default::default()
        };
        let evidence = Evidence {
            facts: &facts,
            evaluations: &[],
        };
        assert_eq!(
            route_decision(&m.routes[1], &evidence),
            "It appears the sender's domain is github.com, and nothing more specific applied, so it is filed under Engineering."
        );
        assert_eq!(
            route_decision(&m.routes[2], &evidence),
            "Nothing above applied, so the judge decides."
        );
    }

    #[test]
    fn facts_lists_negations_and_numbers_read_in_words() {
        let r = route("IF NOT mailing_list AND sender IN never_hidden AND thread_length > 3 AND (routine@v1 < .30 OR novel@v1 >= .5) → Find: Respond + Notify");
        assert_eq!(
            route_reading(&r),
            "When it did not come through a mailing list, the sender is on your Never Hidden list, the thread has more than 3 messages and either routine is unlikely or novel is about as likely as not, it is something to respond to, and you are told at once."
        );
        let r = route("IF NOT (sender_in_contacts OR same_organisation) AND deadline_days <= 2 → Find: Decide");
        assert_eq!(
            route_reading(&r),
            "When neither the sender is in your contacts nor the sender is in your organisation and the deadline is within 2 days, it is something to decide."
        );
        let facts = Facts {
            thread_length: 1,
            ..Default::default()
        };
        let evidence = Evidence {
            facts: &facts,
            evaluations: &[],
        };
        assert_eq!(
            expr_words(
                &route("IF thread_length > 3 → Spoil").condition,
                Some(&evidence)
            ),
            "the thread has one message"
        );
        assert_eq!(
            expr_words(
                &route("IF header X-GitHub-Reason = ci_activity → Spoil").condition,
                None
            ),
            "the x-github-reason header is ci_activity"
        );
    }

    #[test]
    fn a_guard_reads_as_a_veto() {
        let m = parse("Mesh: T\nROUTES\nIF mailing_list → Spoil\nGUARDS\nNEVER Spoil IF sender IN never_hidden\nNEVER Find IF gmail_spam").unwrap();
        assert_eq!(
            guard_reading(&m.guards[0]),
            "Never Spoil when the sender is on your Never Hidden list."
        );
        assert_eq!(
            guard_reading(&m.guards[1]),
            "Never a Find when Gmail marked it spam."
        );
    }

    #[test]
    fn the_parser_fills_the_readings_and_fill_completes_a_stored_mesh() {
        let mut m = parse("Mesh: T\nROUTES\nIF mailing_list → Spoil\nGUARDS\nNEVER Spoil IF sender IN never_hidden").unwrap();
        assert!(m.routes[0]
            .words
            .starts_with("When it came through a mailing list"));
        assert!(m.guards[0].words.starts_with("Never Spoil"));
        m.routes[0].words.clear();
        m.guards[0].words.clear();
        fill(&mut m);
        assert!(!m.routes[0].words.is_empty() && !m.guards[0].words.is_empty());
    }
}
