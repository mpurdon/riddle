//! Deterministic facts (§17): the fixed vocabulary computed without any
//! model. Dates and deadlines are compared here, never asked of the judge,
//! because the vendor reads dates as text (§18). The word lists for urgency
//! and payment/credential asks are deliberately short and literal: they
//! feed the Contamination gate, where a false positive is a message shown as
//! untrusted and a false negative is a phish that reached the judge.

use crate::SiftInput;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use regex::Regex;
use riddle_core::{AuthVerdict, Deadline, Facts};
use std::sync::OnceLock;

const URGENCY: &[&str] = &[
    "urgent",
    "immediately",
    "asap",
    "right away",
    "action required",
    "final notice",
    "last chance",
    "within 24 hours",
    "will be suspended",
    "will be closed",
    "expires today",
    "respond immediately",
    "time sensitive",
    "act now",
    "final warning",
];

const PAYMENT_OR_CREDENTIALS: &[&str] = &[
    "verify your account",
    "confirm your password",
    "enter your password",
    "update your payment",
    "payment information",
    "wire transfer",
    "gift card",
    "bank details",
    "login details",
    "verify your identity",
    "click here to verify",
    "send payment",
    "bitcoin",
    "credit card number",
    "social security",
    "confirm your account",
    "validate your account",
    "reset your password here",
    "your account has been locked",
    "unusual sign-in activity, verify",
];

const BRANDS: &[&str] = &[
    "paypal",
    "google",
    "microsoft",
    "apple",
    "amazon",
    "netflix",
    "dropbox",
    "docusign",
    "github",
    "linkedin",
    "facebook",
    "instagram",
    "chase",
    "wellsfargo",
    "bankofamerica",
    "stripe",
    "shopify",
    "fedex",
    "dhl",
    "usps",
    "ups",
    "irs",
    "office365",
    "outlook",
    "icloud",
    "coinbase",
    "binance",
];

pub fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| text.contains(n))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The label before the public suffix: `mail.paypal-secure.com` → `paypal-secure`.
fn second_level(domain: &str) -> String {
    let parts: Vec<&str> = domain.split('.').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_string(),
        n => {
            // Two-part public suffixes such as co.uk.
            let tld_len = if parts[n - 1].len() == 2 && n >= 3 && parts[n - 2].len() <= 3 {
                2
            } else {
                1
            };
            parts[n.saturating_sub(tld_len + 1)].to_string()
        }
    }
}

fn dehomoglyph(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => 'o',
            '1' | 'l' => 'i',
            '3' => 'e',
            '5' => 's',
            '7' => 't',
            '@' => 'a',
            other => other,
        })
        .collect()
}

/// Whether the sender's domain imitates one of the targets: the same
/// second-level label with digit or letter swaps, one or two edits away, or
/// the target embedded with extra words (`paypal-secure`). An exact match
/// against a target's own domain is never a lookalike.
pub fn is_lookalike(sender_domain: &str, own_domains: &[String]) -> bool {
    let sender = sender_domain.to_ascii_lowercase();
    let label = second_level(&sender);
    if label.is_empty() {
        return false;
    }
    let mut targets: Vec<String> = BRANDS.iter().map(|b| b.to_string()).collect();
    for d in own_domains {
        let l = second_level(&d.to_ascii_lowercase());
        if !l.is_empty() {
            targets.push(l);
        }
    }
    let own_exact = own_domains.iter().any(|d| {
        d.eq_ignore_ascii_case(&sender) || sender.ends_with(&format!(".{}", d.to_ascii_lowercase()))
    });
    if own_exact {
        return false;
    }
    let cleaned = dehomoglyph(&label);
    for t in &targets {
        if &label == t {
            // The brand's own domain (paypal.com) or an unrelated TLD squat;
            // treat the exact label as legitimate.
            continue;
        }
        if &cleaned == t {
            return true;
        }
        if t.len() >= 5 && edit_distance(&label, t) <= if t.len() >= 8 { 2 } else { 1 } {
            return true;
        }
        if t.len() >= 5 && label.contains(t.as_str()) && label.len() > t.len() {
            return true;
        }
    }
    false
}

fn weekday_of(name: &str) -> Option<Weekday> {
    Some(match name {
        "monday" => Weekday::Mon,
        "tuesday" => Weekday::Tue,
        "wednesday" => Weekday::Wed,
        "thursday" => Weekday::Thu,
        "friday" => Weekday::Fri,
        "saturday" => Weekday::Sat,
        "sunday" => Weekday::Sun,
        _ => return None,
    })
}

fn month_of(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    MONTHS
        .iter()
        .position(|m| m.starts_with(&name[..3.min(name.len())]) && name.len() >= 3)
        .map(|i| i as u32 + 1)
}

fn end_of_day(d: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(23, 59, 59).unwrap_or_default())
}

struct Patterns {
    iso: Regex,
    relative_day: Regex,
    weekday: Regex,
    month_day: Regex,
    day_month: Regex,
    within: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        iso: Regex::new(r"\b(?:by|due|before|until|deadline)[:\s]+(?:on\s+)?(\d{4}-\d{2}-\d{2})").unwrap(),
        relative_day: Regex::new(r"\b(?:by|due|before|until|eod|end of day|cob)\s+(?:eod\s+|end of day\s+|cob\s+|end of\s+)?(today|tomorrow)\b|\bby\s+(eod|end of day|cob)\b").unwrap(),
        weekday: Regex::new(r"\b(?:by|due|before|until|on)\s+(?:eod\s+|end of day\s+|cob\s+)?(?:this\s+|next\s+)?(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b").unwrap(),
        month_day: Regex::new(r"\b(?:by|due|before|until|on)\s+(?:the\s+)?(jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\.?\s+(\d{1,2})(?:st|nd|rd|th)?\b").unwrap(),
        day_month: Regex::new(r"\b(?:by|due|before|until|on)\s+(?:the\s+)?(\d{1,2})(?:st|nd|rd|th)?\s+(?:of\s+)?(jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\b").unwrap(),
        within: Regex::new(r"\b(?:due|within|in)\s+(?:the next\s+)?(\d{1,2})\s+(day|days|week|weeks|hour|hours)\b").unwrap(),
    })
}

/// Deadlines parsed from phrases like "by Friday", "due 2026-10-01", "EOD
/// tomorrow", "by October 3", "within 2 days", compared to `now`.
pub fn parse_deadlines(text: &str, now: DateTime<Utc>) -> Vec<Deadline> {
    let lower = text.to_lowercase();
    let p = patterns();
    let today = now.date_naive();
    let mut found: Vec<(DateTime<Utc>, String)> = Vec::new();
    for c in p.iso.captures_iter(&lower) {
        if let Ok(d) = NaiveDate::parse_from_str(&c[1], "%Y-%m-%d") {
            found.push((end_of_day(d), c[0].to_string()));
        }
    }
    for c in p.relative_day.captures_iter(&lower) {
        let word = c.get(1).map(|m| m.as_str()).unwrap_or("today");
        let d = if word == "tomorrow" {
            today + Duration::days(1)
        } else {
            today
        };
        found.push((end_of_day(d), c[0].to_string()));
    }
    for c in p.weekday.captures_iter(&lower) {
        if let Some(wd) = weekday_of(&c[1]) {
            let mut d = today;
            let next = c[0].contains("next ");
            let mut steps = 0;
            while d.weekday() != wd || (next && steps == 0) {
                d += Duration::days(1);
                steps += 1;
                if steps > 14 {
                    break;
                }
            }
            found.push((end_of_day(d), c[0].to_string()));
        }
    }
    let mut month_day_hits: Vec<(u32, u32, String)> = Vec::new();
    for c in p.month_day.captures_iter(&lower) {
        if let (Some(m), Ok(day)) = (month_of(&c[1]), c[2].parse::<u32>()) {
            month_day_hits.push((m, day, c[0].to_string()));
        }
    }
    for c in p.day_month.captures_iter(&lower) {
        if let (Ok(day), Some(m)) = (c[1].parse::<u32>(), month_of(&c[2])) {
            month_day_hits.push((m, day, c[0].to_string()));
        }
    }
    for (m, day, text) in month_day_hits {
        let mut year = today.year();
        if let Some(d) = NaiveDate::from_ymd_opt(year, m, day) {
            // A date more than 60 days in the past means next year.
            if (today - d).num_days() > 60 {
                year += 1;
            }
            if let Some(d) = NaiveDate::from_ymd_opt(year, m, day) {
                found.push((end_of_day(d), text));
            }
        }
    }
    for c in p.within.captures_iter(&lower) {
        if let Ok(n) = c[1].parse::<i64>() {
            let at = match &c[2] {
                "hour" | "hours" => now + Duration::hours(n),
                "week" | "weeks" => end_of_day(today + Duration::weeks(n)),
                _ => end_of_day(today + Duration::days(n)),
            };
            found.push((at, c[0].to_string()));
        }
    }
    let mut out: Vec<Deadline> = Vec::new();
    for (at, text) in found {
        if out.iter().any(|d| d.at == at) {
            continue;
        }
        let diff = at - now;
        out.push(Deadline {
            at,
            text,
            days_from_now: diff.num_days(),
            past: at < now,
        });
    }
    out.sort_by_key(|d| d.at);
    out
}

/// Compute every fact from the input; nothing here calls a model.
pub fn compute_facts(input: &SiftInput, now: DateTime<Utc>) -> Facts {
    let m = &input.message;
    let profile = &input.profile;
    let sender = m.from.email.to_ascii_lowercase();
    let sender_domain = m.from.domain().to_ascii_lowercase();
    let text = format!("{}\n{}", m.subject, input.normalized_text).to_lowercase();
    let directly_addressed = m.to.iter().any(|a| profile.is_me(&a.email));
    let cc_only = !directly_addressed && m.cc.iter().any(|a| profile.is_me(&a.email));
    let precedence = m
        .headers
        .precedence
        .as_deref()
        .map(|p| p.to_ascii_lowercase())
        .unwrap_or_default();
    let auth = &m.headers.authentication;
    let reply_to_mismatch = m
        .headers
        .reply_to
        .as_ref()
        .map(|r| !r.domain().eq_ignore_ascii_case(&sender_domain))
        .unwrap_or(false);
    let labels_upper: Vec<String> = m.labels.iter().map(|l| l.to_ascii_uppercase()).collect();
    Facts {
        sender_domain: sender_domain.clone(),
        sender_in_contacts: input.people.in_contacts,
        sender_replied_to_before: input.people.replied_to_before,
        first_time_sender: input.people.first_time_sender,
        same_organisation: profile.is_own_organisation(&sender),
        mailing_list: m.headers.list_id.is_some() || input.people.mailing_list,
        list_unsubscribe: m.headers.list_unsubscribe.is_some(),
        precedence_bulk: matches!(precedence.as_str(), "bulk" | "list" | "junk"),
        auto_submitted: m
            .headers
            .auto_submitted
            .as_deref()
            .is_some_and(|v| !v.eq_ignore_ascii_case("no")),
        spf_pass: auth.spf == AuthVerdict::Pass,
        dkim_pass: auth.dkim == AuthVerdict::Pass,
        dmarc_pass: auth.dmarc == AuthVerdict::Pass,
        reply_to_mismatch,
        gmail_category: input
            .gmail_categories
            .first()
            .map(|c| c.to_ascii_lowercase()),
        gmail_important: labels_upper.iter().any(|l| l == "IMPORTANT"),
        gmail_spam: labels_upper.iter().any(|l| l == "SPAM"),
        directly_addressed,
        cc_only,
        thread_length: input
            .thread_state
            .as_ref()
            .map(|t| t.message_count.max(1))
            .unwrap_or(1),
        my_last_reply_at: input
            .thread_state
            .as_ref()
            .and_then(|t| t.last_reply_by_me_at),
        has_calendar_part: input.has_calendar_part,
        has_structured_receipt: input.has_structured_receipt,
        has_attachments: !m.attachments.is_empty(),
        language: if input.language.trim().is_empty() {
            "und".into()
        } else {
            input.language.to_ascii_lowercase()
        },
        urgency_language_present: contains_any(&text, URGENCY),
        asks_for_payment_or_credentials: contains_any(&text, PAYMENT_OR_CREDENTIALS),
        deadlines: parse_deadlines(&text, now),
        is_sent: m.is_sent,
        lookalike_domain: is_lookalike(&sender_domain, &profile.organisation_domains),
        sender_in_never_hidden: profile.is_never_hidden(&sender),
        sender_is_vip: profile.is_vip(&sender),
    }
}

/// Gmail category names from provider labels: `CATEGORY_PROMOTIONS` → `promotions`.
pub fn gmail_categories_from_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .filter_map(|l| {
            l.to_ascii_uppercase()
                .strip_prefix("CATEGORY_")
                .map(|c| c.to_ascii_lowercase())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadlines_are_parsed_and_compared_to_now() {
        // 2026-09-22 is a Tuesday.
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0).unwrap();
        let d = parse_deadlines("Please send it by Friday. The report is due 2026-10-01 and the invoice by EOD tomorrow.", now);
        let texts: Vec<&str> = d.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(d.len(), 3, "{texts:?}");
        assert_eq!(
            d[0].at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 23).unwrap(),
            "tomorrow"
        );
        assert_eq!(
            d[1].at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 25).unwrap(),
            "friday"
        );
        assert_eq!(
            d[2].at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
        );
        assert!(d.iter().all(|x| !x.past));
        assert_eq!(d[1].days_from_now, 3);
        let past = parse_deadlines("payment was due 2026-09-01", now);
        assert!(past[0].past);
        let month = parse_deadlines(
            "RSVP by October 3rd; slides due on the 5th of november; reply within 2 days",
            now,
        );
        assert_eq!(month.len(), 3);
        assert_eq!(
            month[0].at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 24).unwrap()
        );
        assert_eq!(
            month[1].at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()
        );
        assert_eq!(
            month[2].at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
        );
    }

    #[test]
    fn lookalike_domains_are_detected_and_real_ones_are_not() {
        let own = vec!["northwindservices.com".to_string()];
        assert!(is_lookalike("paypa1.com", &own));
        assert!(is_lookalike("paypal-secure.com", &own));
        assert!(is_lookalike("mail.northwindservlces.com", &own));
        assert!(is_lookalike("micros0ft.com", &own));
        assert!(!is_lookalike("paypal.com", &own));
        assert!(!is_lookalike("northwindservices.com", &own));
        assert!(!is_lookalike("mail.northwindservices.com", &own));
        assert!(!is_lookalike("example.org", &own));
        assert!(!is_lookalike("github.com", &own));
    }
}
