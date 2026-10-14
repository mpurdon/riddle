//! Deterministic facts (§17): the fixed vocabulary computed without any
//! model. Dates and deadlines are compared here, never asked of the judge,
//! because the vendor reads dates as text (§18). The word lists for urgency
//! and payment/credential asks are deliberately short and literal: they
//! feed the Contamination gate, where a false positive is a message shown as
//! untrusted and a false negative is a phish that reached the judge.

use crate::SiftInput;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use regex::Regex;
pub use riddle_core::free_mail_domain;
use riddle_core::{AuthVerdict, Deadline, Facts, Message};
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

/// Domains a brand sends from besides its own name: Amazon Web Services
/// mails from amazonaws.com. An embedded brand reads as imitation, so these
/// are named.
const OFFICIAL_DOMAINS: &[&str] = &[
    "amazonaws.com",
    "amazonses.com",
    "amazonmusic.com",
    "amazon-adsystem.com",
    "googlemail.com",
    "microsoftonline.com",
    "paypal-communication.com",
    "paypal-communications.com",
];

/// Whether the sender's domain imitates one of the targets: the same
/// second-level label with digit or letter swaps, one or two edits away, or
/// the target set apart inside a longer name (`paypal-secure`,
/// `secure-paypal`, `paypal1`). An exact match against a target's own
/// domain is never a lookalike. The embedded brand has to stand as its own
/// word: on a real mailbox every one of twelve flagged messages was a false
/// hit (`amazonaws.com` "imitating" Amazon, `stripedseed.ca` "imitating"
/// Stripe), and the flag sends mail to Contamination without the judge, so
/// four genuine AWS past-due notices were hidden.
pub fn is_lookalike(sender_domain: &str, own_domains: &[String]) -> bool {
    let sender = sender_domain.to_ascii_lowercase();
    if OFFICIAL_DOMAINS.iter().any(|d| is_or_under(&sender, d)) {
        return false;
    }
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
    let own_exact = own_domains
        .iter()
        .any(|d| is_or_under(&sender, &d.to_ascii_lowercase()));
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
        if t.len() >= 5 && label.len() > t.len() && embedded_as_word(&label, t) {
            return true;
        }
    }
    false
}

/// `t` appears in `label` with a separator, a digit or the label's edge on
/// both sides: `paypal-secure`, `my-paypal`, `paypal1`, not `amazonaws`.
fn embedded_as_word(label: &str, t: &str) -> bool {
    let apart = |c: Option<char>| c.is_none_or(|c| c == '-' || c.is_ascii_digit());
    label.match_indices(t).any(|(i, _)| {
        apart(label[..i].chars().next_back()) && apart(label[i + t.len()..].chars().next())
    })
}

/// The community a message comes from, when it is a post or an update from
/// a group the user belongs to rather than a platform's own marketing:
/// `nextdoor`, `facebook_group`, `google_group`, `groups_io`, `meetup`, or
/// `creator` for posts from creators the user backs on Patreon; empty
/// otherwise. On one mailbox Nextdoor's neighbourhood posts were filed with
/// LinkedIn and Pinterest, two were made Pay Finds ("Jon owes me $2500"),
/// and Patreon posts were spoiled as promotions. A platform's own mail is
/// not a community: Nextdoor markets from `ms.` hosts, Patreon from
/// `updates.patreon.com`.
pub fn community_platform(
    sender: &str,
    sender_name: &str,
    sender_domain: &str,
    list_id: &str,
) -> String {
    let domain = sender_domain.to_ascii_lowercase();
    let local = sender.split('@').next().unwrap_or("").to_ascii_lowercase();
    let name = sender_name.trim();
    let platform = if is_or_under(&domain, "nextdoor.com") || is_or_under(&domain, "nextdoor.ca") {
        (!domain.split('.').any(|p| p == "ms")).then_some("nextdoor")
    } else if domain == "facebookmail.com" {
        // A group's notifications carry the group's name ("Lakeview
        // Residents"); Facebook's own carry "Facebook".
        (local.starts_with("groupupdates")
            || (local == "notification"
                && !name.is_empty()
                && !name.eq_ignore_ascii_case("facebook")))
        .then_some("facebook_group")
    } else if list_id.ends_with("googlegroups.com") {
        Some("google_group")
    } else if list_id.ends_with("groups.io") || is_or_under(&domain, "groups.io") {
        Some("groups_io")
    } else if is_or_under(&domain, "meetup.com") {
        Some("meetup")
    } else if is_or_under(&domain, "creator.patreon.com") || domain == "info.patreon.com" {
        Some("creator")
    } else {
        None
    };
    platform.unwrap_or_default().to_string()
}

/// A payment that did not go through: failed, declined, unsuccessful, or a
/// request for a different card. It reads the subject and the start of the
/// body, and a subject that says the payment succeeded wins, so "Payment
/// attempt successful on backup payment method" is not a failure. On one
/// mailbox ten such notices (Google Workspace, Stripe's "was
/// unsuccessful", Uber) were plain Pay Finds at the same priority as a
/// bill, though each one meant a service was about to lapse.
pub fn payment_failed(subject: &str, body: &str) -> bool {
    let subject = subject.to_lowercase();
    if payment_succeeded(&subject) {
        return false;
    }
    let lead: String = body.chars().take(600).collect::<String>().to_lowercase();
    FAILED.iter().any(|p| subject.contains(p))
        || (subject.contains("payment") && FAILED.iter().any(|p| lead.contains(p)))
}

/// A payment that went through on a backup method after the main one
/// failed: nothing to pay, but the main card wants attention.
pub fn payment_recovered(subject: &str, body: &str) -> bool {
    let subject = subject.to_lowercase();
    let lead: String = body.chars().take(600).collect::<String>().to_lowercase();
    payment_succeeded(&subject)
        && [
            "backup payment",
            "backup card",
            "alternate payment",
            "secondary payment",
            "other payment method",
        ]
        .iter()
        .any(|p| subject.contains(p) || lead.contains(p))
}

const FAILED: &[&str] = &[
    "payment failed",
    "payment failure",
    "failed payment",
    "payment was declined",
    "payment declined",
    "card was declined",
    "card declined",
    "was unsuccessful",
    "payment unsuccessful",
    "payment was not successful",
    "unable to process your payment",
    "unable to charge",
    "couldn't process your payment",
    "could not process your payment",
    "couldn't charge",
    "could not charge",
    "problem with your payment",
    "add a different payment method",
    "update your payment method",
    "update your payment details",
];

fn payment_succeeded(subject: &str) -> bool {
    let unsuccessful = subject.contains("unsuccessful");
    (!unsuccessful && subject.contains("successful"))
        || subject.contains("succeeded")
        || subject.contains("payment received")
        || subject.contains("receipt")
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

/// What happened, for a GitHub notification: the first line of the body
/// (or the subject, for Actions) names the event. `X-GitHub-Reason` says
/// why the user is on the thread, and it sticks: once a team is asked to
/// review a PR, every approval, bot comment and merge on it arrives as
/// `review_requested`. On one mailbox 1,254 of 1,440 such mails were thread
/// follow-ups and 32 asked the user personally, so a rule needs the event
/// as well as the reason. Values: `review_requested_you`,
/// `review_requested_team`, `changes_requested`, `approved`, `commented`,
/// `merged`, `closed`, `pushed`, `run_failed`, `opened`; empty otherwise.
pub fn github_event(sender_domain: &str, subject: &str, body: &str) -> Option<String> {
    if !(sender_domain == "github.com" || sender_domain.ends_with(".github.com")) {
        return None;
    }
    if subject.contains("Run failed:") {
        return Some("run_failed".into());
    }
    let first = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let event = [
        ("requested your review", "review_requested_you"),
        ("requested review from @", "review_requested_team"),
        (
            "requested changes on this pull request",
            "changes_requested",
        ),
        ("approved this pull request", "approved"),
        ("commented on this pull request", "commented"),
        (" left a comment (", "commented"),
        ("Merged #", "merged"),
        ("Closed #", "closed"),
        (" pushed ", "pushed"),
    ]
    .iter()
    .find(|(phrase, _)| first.contains(phrase))
    .map(|(_, e)| e.to_string());
    // A new pull request leads with its own description, then this line.
    event.or_else(|| {
        body.contains("You can view, comment on, or merge this pull request online at:")
            .then(|| "opened".to_string())
    })
}

/// The one-time code a sign-in or verification message carries, or empty.
/// A code is 4 to 10 digits (Keeper's are 10) next to a phrase that introduces one: a strong
/// phrase ("verification code", "single-use code", "sign-in code",
/// "passcode") on either side, since a subject can lead with the code
/// ("035277 is your Kickstarter sign-in code"); a weak one ("your FreshBooks
/// code", "use this code", "the code below") only before it. Of a real
/// mailbox's code mail, the traps were the footer's postal code ("WA
/// 98052"), numbers inside links, years, prices, phone numbers, and the
/// word "code" in newsletters ("Claude Code"), so links and addresses are
/// removed first, the nearest candidate wins, and a booking's
/// "confirmation code" or a "promo code" is not a sign-in code.
pub fn verification_code(subject: &str, body: &str) -> String {
    struct Patterns {
        strip: Regex,
        strong: Regex,
        weak: Regex,
        digits: Regex,
    }
    static P: OnceLock<Patterns> = OnceLock::new();
    let p = P.get_or_init(|| Patterns {
        strip: Regex::new(r"(?i)\bhttps?://\S+|\bwww\.\S+|\S+@\S+\.\S+").unwrap(),
        strong: Regex::new(r"(?i)\b(?:(?:verification|security|sign[\s-]?in|log[\s-]?in|login|one[\s-]?time|single[\s-]?use|authentication|2fa|mfa|otp)\s+(?:pass)?code|pass\s?code|one[\s-]?time\s+password|otp)\b").unwrap(),
        weak: Regex::new(r"(?i)\b(?:(?:your|this|the)\s+(?:[a-z][\w.-]*\s+){0,2}code|code\s+(?:is|below))\b").unwrap(),
        digits: Regex::new(r"\d{4,10}").unwrap(),
    });
    const NOT_SIGN_IN: [&str; 12] = [
        "confirmation",
        "promo",
        "discount",
        "coupon",
        "gift",
        "access",
        "zip",
        "postal",
        "source",
        "claude",
        "qr",
        "dress",
    ];
    let body: String = body.chars().take(4000).collect();
    let text = p
        .strip
        .replace_all(&format!("{subject}\n{body}"), " ")
        .into_owned();
    // A standalone run of digits: no digit, letter or price/percent mark
    // against it, and not one part of a phone number or a date.
    let candidates: Vec<(usize, usize)> = p
        .digits
        .find_iter(&text)
        .filter(|m| {
            let before = text[..m.start()].chars().next_back();
            let after = text[m.end()..].chars().next();
            let before2 = text[..m.start()].chars().rev().nth(1);
            let after2 = text[m.end()..].chars().nth(1);
            let joined =
                |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || "$€£#%+".contains(c));
            let grouped = |sep: Option<char>, next: Option<char>| {
                sep.is_some_and(|c| "-./:,".contains(c)) && next.is_some_and(|c| c.is_ascii_digit())
            };
            let year =
                m.len() == 4 && (m.as_str().starts_with("19") || m.as_str().starts_with("20"));
            // A US ZIP after its state: "Chicago IL 60606".
            let zip = m.len() == 5 && {
                let lead = text[..m.start()].trim_end();
                let state: String = lead
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphabetic())
                    .collect();
                state.len() == 2 && state.chars().all(|c| c.is_ascii_uppercase())
            };
            !joined(before)
                && !joined(after)
                && !grouped(before, before2)
                && !grouped(after, after2)
                && !year
                && !zip
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    if candidates.is_empty() {
        return String::new();
    }
    let lower = text.to_lowercase();
    let mut best: Option<(usize, (usize, usize))> = None;
    for (re, both_sides) in [(&p.strong, true), (&p.weak, false)] {
        for t in re.find_iter(&text) {
            // The two words before the phrase, too: "your promo code".
            let lead = lower[..t.start()]
                .rsplitn(3, char::is_whitespace)
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            let phrase = &lower[t.start()..t.end()];
            if NOT_SIGN_IN
                .iter()
                .any(|w| phrase.contains(w) || lead.contains(w))
            {
                continue;
            }
            for &(s, e) in &candidates {
                let distance = if s >= t.end() {
                    s - t.end()
                } else if both_sides && e <= t.start() {
                    t.start() - e
                } else {
                    continue;
                };
                if distance <= 200 && best.is_none_or(|(d, _)| distance < d) {
                    best = Some((distance, (s, e)));
                }
            }
        }
        // A strong phrase's code wins over a nearer weak one.
        if best.is_some() {
            break;
        }
    }
    best.map(|(_, (s, e))| text[s..e].to_string())
        .unwrap_or_default()
}

/// The mailing list identifier (RFC 2919), the part in angle brackets of
/// `List-Id`: `platform.northwind.github.com` for a GitHub repository,
/// lowercase. Empty when the mail is not from a list.
pub fn list_id(headers: &riddle_core::MessageHeaders) -> String {
    let Some(raw) = headers.list_id.as_deref() else {
        return String::new();
    };
    let id = match (raw.rfind('<'), raw.rfind('>')) {
        (Some(a), Some(b)) if b > a => &raw[a + 1..b],
        _ => raw,
    };
    id.trim().to_ascii_lowercase()
}

/// GitHub says why it sent a notification in `X-GitHub-Reason`. Mail synced
/// before that header was kept carries the same fact in its footer ("You are
/// receiving this because your review was requested."), so a rule reads
/// either, and a Signal is never asked what a header states. Of one
/// mailbox's 2,300 GitHub notifications, 2,050 were Unresolved because the
/// only route for them asked the LLM.
pub fn github_reason(
    headers: &riddle_core::MessageHeaders,
    lowercase_text: &str,
) -> Option<String> {
    if let Some(r) = headers.extra.get("x-github-reason") {
        let r = r.trim().to_ascii_lowercase();
        if !r.is_empty() {
            return Some(r);
        }
    }
    const FOOTER: &str = "you are receiving this because ";
    let start = lowercase_text.find(FOOTER)? + FOOTER.len();
    let rest = lowercase_text[start..].split(['.', '\n']).next()?.trim();
    let reason = [
        ("your review was requested", "review_requested"),
        ("you are subscribed", "subscribed"),
        ("you are watching", "subscribed"),
        ("you commented", "comment"),
        ("you were mentioned", "mention"),
        ("your team was mentioned", "team_mention"),
        ("you are on a team that was mentioned", "team_mention"),
        ("you authored", "author"),
        ("you were assigned", "assign"),
        ("you modified the open/close state", "state_change"),
        ("you pushed", "push"),
        ("you are the administrator", "manual"),
        ("you are an administrator", "manual"),
    ]
    .iter()
    .find(|(phrase, _)| rest.starts_with(phrase))
    .map(|(_, reason)| reason.to_string())?;
    Some(reason)
}

/// What a stored fact needs besides the message: its detected language and
/// the sender's rhythm from People.
#[derive(Debug, Clone, Copy, Default)]
pub struct Derived<'a> {
    pub language: &'a str,
    pub cadence: Option<riddle_core::Cadence>,
    pub on_schedule: bool,
    /// The From address is one of the user's (`UserProfile::is_me`, with
    /// the Dig's own address counted).
    pub me: bool,
    /// The user's organisation domains, which a lookalike may imitate.
    pub organisation_domains: &'a [String],
    /// The thread and the sender's domain are ones the user already knows
    /// (`Facts::known_thread`, `Facts::known_domain`), from the stores.
    pub known_thread: bool,
    pub known_domain: bool,
}

/// The facts that come from the message, its language and its sender's
/// rhythm, set on `facts`; true when any of them changed. `compute_facts`
/// uses it, and the boot-time refresh runs it over facts stored before a
/// field existed (`sender`, `subject`, `headers`, `github_reason`, then a
/// fixed `language`, then `sender_cadence`), so a rule on them simulates
/// over history rather than over empty strings or yesterday's detector. The
/// GitHub footer is read from the raw body: it sits where normalisation
/// strips signatures.
pub fn refresh_from_message(facts: &mut Facts, m: &Message, derived: Derived<'_>) -> bool {
    let body = m.body_text.as_deref().unwrap_or(&m.snippet);
    let text = format!("{}\n{body}", m.subject).to_lowercase();
    let mut changed = false;
    fn set<T: PartialEq>(slot: &mut T, value: T, changed: &mut bool) {
        if *slot != value {
            *slot = value;
            *changed = true;
        }
    }
    set(
        &mut facts.sender,
        m.from.email.to_ascii_lowercase(),
        &mut changed,
    );
    set(
        &mut facts.sender_domain,
        m.from.domain().to_ascii_lowercase(),
        &mut changed,
    );
    set(
        &mut facts.sender_name,
        m.from.name.clone().unwrap_or_default(),
        &mut changed,
    );
    set(&mut facts.subject, m.subject.clone(), &mut changed);
    set(&mut facts.known_thread, derived.known_thread, &mut changed);
    set(&mut facts.known_domain, derived.known_domain, &mut changed);
    set(&mut facts.headers, m.headers.extra.clone(), &mut changed);
    set(
        &mut facts.github_reason,
        github_reason(&m.headers, &text),
        &mut changed,
    );
    set(
        &mut facts.github_event,
        github_event(&facts.sender_domain, &m.subject, body),
        &mut changed,
    );
    set(&mut facts.list_id, list_id(&m.headers), &mut changed);
    set(
        &mut facts.verification_code,
        verification_code(&m.subject, body),
        &mut changed,
    );
    set(
        &mut facts.payment_failed,
        payment_failed(&m.subject, body),
        &mut changed,
    );
    set(
        &mut facts.payment_recovered,
        payment_recovered(&m.subject, body),
        &mut changed,
    );
    set(
        &mut facts.community_platform,
        community_platform(
            &facts.sender,
            &facts.sender_name,
            &facts.sender_domain,
            &facts.list_id,
        ),
        &mut changed,
    );
    let lookalike = is_lookalike(&facts.sender_domain, derived.organisation_domains);
    set(&mut facts.lookalike_domain, lookalike, &mut changed);
    let language = derived.language.trim();
    set(
        &mut facts.language,
        if language.is_empty() {
            "und".into()
        } else {
            language.to_ascii_lowercase()
        },
        &mut changed,
    );
    set(
        &mut facts.sender_cadence,
        derived
            .cadence
            .map(|c| c.as_str().to_string())
            .unwrap_or_default(),
        &mut changed,
    );
    set(
        &mut facts.sender_on_schedule,
        derived.on_schedule,
        &mut changed,
    );
    set(
        &mut facts.government_sender,
        government_domain(&facts.sender_domain)
            && m.headers.authentication.dmarc == riddle_core::AuthVerdict::Pass,
        &mut changed,
    );
    let is_draft = m.labels.iter().any(|l| l.eq_ignore_ascii_case("DRAFT"));
    set(&mut facts.is_draft, is_draft, &mut changed);
    set(
        &mut facts.is_reply,
        m.headers.in_reply_to.is_some() || !m.headers.references.is_empty(),
        &mut changed,
    );
    let from_me = derived.me
        && (m.is_sent || is_draft || m.headers.authentication.dmarc == AuthVerdict::Pass);
    set(&mut facts.from_me, from_me, &mut changed);
    set(
        &mut facts.spoofs_me,
        derived.me
            && !m.is_sent
            && !is_draft
            && m.headers.authentication.dmarc == AuthVerdict::Fail,
        &mut changed,
    );
    // The user is never a first-time sender to themselves; People counts
    // drafts as mail from them, one at a time.
    if from_me {
        set(&mut facts.first_time_sender, false, &mut changed);
    }
    set(
        &mut facts.free_mail_sender,
        free_mail_domain(&facts.sender_domain),
        &mut changed,
    );
    set(
        &mut facts.suspicious_domain,
        suspicious_domain(&facts.sender_domain),
        &mut changed,
    );
    set(
        &mut facts.glyph_substitution,
        glyph_substitution(&format!("{}\n{body}", m.subject)),
        &mut changed,
    );
    changed
}

/// Generic top-level domains mail really comes from. Two-letter endings are
/// country codes and always count as real; a longer ending outside this
/// list (`.zbc`, `.jzl`) is the kind phishing kits generate. Not the whole
/// root zone: a real but rare ending reads as suspicious, which is why the
/// rule that uses it only Spoils, and only a stranger, unless the mail also
/// asks for payment or credentials.
const REAL_TLDS: &[&str] = &[
    "com",
    "net",
    "org",
    "edu",
    "gov",
    "mil",
    "int",
    "info",
    "biz",
    "name",
    "pro",
    "aero",
    "coop",
    "museum",
    "mobi",
    "asia",
    "tel",
    "travel",
    "jobs",
    "app",
    "dev",
    "page",
    "io",
    "ai",
    "xyz",
    "online",
    "site",
    "store",
    "shop",
    "club",
    "live",
    "email",
    "news",
    "cloud",
    "tech",
    "blog",
    "link",
    "click",
    "space",
    "website",
    "world",
    "today",
    "life",
    "art",
    "design",
    "agency",
    "digital",
    "media",
    "network",
    "solutions",
    "services",
    "support",
    "systems",
    "tools",
    "studio",
    "center",
    "company",
    "group",
    "global",
    "team",
    "works",
    "zone",
    "academy",
    "school",
    "university",
    "college",
    "education",
    "care",
    "health",
    "clinic",
    "money",
    "finance",
    "financial",
    "bank",
    "insurance",
    "capital",
    "fund",
    "investments",
    "credit",
    "loans",
    "tax",
    "law",
    "legal",
    "consulting",
    "marketing",
    "events",
    "photography",
    "photos",
    "video",
    "film",
    "music",
    "game",
    "games",
    "fun",
    "bet",
    "casino",
    "social",
    "chat",
    "community",
    "forum",
    "reviews",
    "guide",
    "wiki",
    "one",
    "top",
    "vip",
    "win",
    "best",
    "city",
    "town",
    "land",
    "house",
    "homes",
    "realty",
    "rentals",
    "vacations",
    "holiday",
    "tours",
    "flights",
    "hotel",
    "cars",
    "auto",
    "fitness",
    "golf",
    "football",
    "soccer",
    "sport",
    "fashion",
    "style",
    "clothing",
    "shoes",
    "jewelry",
    "beauty",
    "pizza",
    "restaurant",
    "cafe",
    "coffee",
    "wine",
    "beer",
    "kitchen",
    "recipes",
    "garden",
    "farm",
    "energy",
    "solar",
    "green",
    "eco",
    "earth",
    "science",
    "technology",
    "software",
    "computer",
    "hosting",
    "host",
    "mail",
    "inc",
    "llc",
    "ltd",
    "gmbh",
    "foundation",
    "ngo",
    "charity",
    "church",
    "family",
    "kids",
    "pet",
    "vet",
    "training",
    "courses",
    "run",
    "show",
    "nyc",
    "london",
    "paris",
    "berlin",
    "tokyo",
    "amsterdam",
    "toronto",
    "vegas",
    "quebec",
    "google",
    "goog",
    "youtube",
    "apple",
    "amazon",
    "microsoft",
    "aws",
    "ninja",
    "rocks",
    "guru",
    "expert",
    "email",
    "plus",
    "express",
    "direct",
    "market",
    "sale",
    "deals",
    "shopping",
    "gift",
    "gifts",
    "ticket",
    "tickets",
    "film",
    "tv",
    "radio",
    "press",
    "report",
    "foo",
    "new",
    "here",
    "how",
    "now",
    "ink",
    "lol",
    "wtf",
    "gay",
];

/// A throwaway sender domain (`Facts::suspicious_domain`): an ending that is
/// not a real top-level domain, or a name that is a random string: at least
/// two digits beside a run of four consonants (`628ydjfg`), or five or more
/// letters without a vowel. Brands with digits (`1800flowers`,
/// `99designs`) keep their vowels.
pub fn suspicious_domain(domain: &str) -> bool {
    let d = domain.trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = d.split('.').filter(|l| !l.is_empty()).collect();
    let Some(tld) = labels.last() else {
        return false;
    };
    if labels.len() < 2 {
        return false;
    }
    let real_tld = (tld.len() == 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
        || REAL_TLDS.contains(tld)
        || tld.starts_with("xn--");
    if !real_tld {
        return true;
    }
    // The registered name: the label before the ending, or before a
    // two-part country ending such as co.uk.
    let name = if labels.len() >= 3
        && tld.len() == 2
        && matches!(
            labels[labels.len() - 2],
            "co" | "com" | "org" | "net" | "gov" | "ac" | "edu"
        ) {
        labels[labels.len() - 3]
    } else {
        labels[labels.len() - 2]
    };
    let vowel = |c: char| "aeiou".contains(c);
    let letters: Vec<char> = name.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let digits = name.chars().filter(|c| c.is_ascii_digit()).count();
    let mut run = 0;
    let mut longest = 0;
    for c in name.chars() {
        if c.is_ascii_alphabetic() && !vowel(c) {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    (digits >= 2 && longest >= 4) || (letters.len() >= 5 && !letters.iter().any(|c| vowel(*c)))
}

/// Letters disguised to get past a filter (see `Facts::glyph_substitution`):
/// a pipe or a lone `l` standing for the pronoun I before a verb ("Thank
/// you, and | look forward"), or a word mixing Latin with Cyrillic letters
/// (`Pаypal` with a Cyrillic а). Two tells were tried and dropped against
/// a real mailbox: zero-width characters inside words, which retailers and
/// Anthropic's own mail use to stop mail apps turning words into links (73
/// of 77 hits), and Greek letters beside Latin, which is every `14μm`.
pub fn glyph_substitution(text: &str) -> bool {
    static PRONOUN: OnceLock<Regex> = OnceLock::new();
    let pronoun = PRONOUN.get_or_init(|| {
        Regex::new(r"(?:^|[\s,.;:!?(])[|l]\s+(?:am|have|hope|look|looking|would|will|was|can|just|wanted|noticed|felt|feel|think|want|need|believe|trust|appreciate|recently|saw|came|found)\b")
            .expect("static pattern")
    });
    pronoun.is_match(text)
        || text.split(|c: char| !c.is_alphabetic()).any(|word| {
            word.chars().any(|c| c.is_ascii_alphabetic())
                && word.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
        })
}

/// Canadian federal, provincial and territorial government domains; mail
/// from these or their subdomains is the government's. `canada.ca` is not
/// matched as a suffix of a longer name: `petro-canada.ca` is a gas station.
const CANADIAN_GOVERNMENT: &[&str] = &[
    "canada.ca",
    "gc.ca",
    "ontario.ca",
    "gov.on.ca",
    "gov.bc.ca",
    "alberta.ca",
    "gov.ab.ca",
    "saskatchewan.ca",
    "gov.sk.ca",
    "manitoba.ca",
    "gov.mb.ca",
    "quebec.ca",
    "gouv.qc.ca",
    "gnb.ca",
    "novascotia.ca",
    "gov.ns.ca",
    "princeedwardisland.ca",
    "gov.pe.ca",
    "gov.nl.ca",
    "yukon.ca",
    "gov.yk.ca",
    "gov.nt.ca",
    "gov.nu.ca",
];

/// Whether a domain belongs to a government: a Canadian one above, a `.gov`
/// or `.mil` domain, or a government second level under a country code
/// (`gov.uk`, `gov.ee`, `gouv.fr`, `govt.nz`, `gob.mx`). Domain boundaries
/// matter; the whole label must match. The Canada Revenue Agency and the
/// RCMP write from `cra-arc.gc.ca` and `rcmp-grc.gc.ca`; the Government of
/// Canada's notices from `notification.canada.ca`.
pub fn government_domain(domain: &str) -> bool {
    let d = domain.trim_end_matches('.');
    if CANADIAN_GOVERNMENT.iter().any(|s| is_or_under(d, s)) {
        return true;
    }
    let mut labels = d.rsplit('.');
    match (labels.next(), labels.next()) {
        (Some("gov" | "mil"), Some(_)) => true,
        (Some(cc), Some(second)) if cc.len() == 2 => {
            matches!(second, "gov" | "gouv" | "govt" | "gob" | "gv")
        }
        _ => false,
    }
}

/// `domain` is `suffix` or a subdomain of it, by whole labels: `a.gc.ca` is
/// under `gc.ca`, `petro-canada.ca` is not under `canada.ca`. Both lowercase.
fn is_or_under(domain: &str, suffix: &str) -> bool {
    domain
        .strip_suffix(suffix)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
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
    let mut facts = Facts {
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
        urgency_language_present: contains_any(&text, URGENCY),
        asks_for_payment_or_credentials: contains_any(&text, PAYMENT_OR_CREDENTIALS),
        deadlines: parse_deadlines(&text, now),
        is_sent: m.is_sent,
        sender_in_never_hidden: profile.is_never_hidden(&sender),
        sender_is_vip: profile.is_vip(&sender),
        ..Default::default()
    };
    refresh_from_message(
        &mut facts,
        m,
        Derived {
            language: &input.language,
            cadence: input.people.cadence,
            on_schedule: input.people.on_schedule,
            me: profile.is_me(&sender),
            organisation_domains: &profile.organisation_domains,
            known_thread: input.people.known_thread,
            known_domain: input.people.known_domain,
        },
    );
    facts
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

    #[test]
    fn a_sign_in_code_is_found_beside_its_phrase_whichever_side_it_is_on() {
        let cases = [
            ("035277 is your Kickstarter sign-in code", "Use this code to continue logging in to your account:\n\n035277\n\nThis verification code expires in 10 minutes.", "035277"),
            ("MFA Login Authentication", "Your Ubiquiti verification code is: KH - 472255.\n\nThis code is only valid for 10 minutes.\nRequested by Network iOS  203.0.113.7", "472255"),
            ("Your FreshBooks code - 038853", "Here is your code This code will expire in 10 minutes. 038853 If you didn't attempt to log in", "038853"),
            ("Your single-use code", "Hi me@gmail.com,\nYour single-use code is: 472839\nPrivacy Statement: https://go.microsoft.com/fwlink/?LinkId=521839 Microsoft Corporation, One Microsoft Way, Redmond, WA 98052", "472839"),
            ("Sign in to Trakt", "Please sign into your account by clicking the button below. Sign In Alternatively, paste the code below into the Trakt website. 307744", "307744"),
            ("Verify your identity with My TELUS", "Your one-time verification code to log into your My TELUS account me@gmail.com is:  902885  This code expires in 30 minutes.", "902885"),
            ("G-482910 is your Google verification code", "", "482910"),
            ("Keeper Security Alert >> Login Approval Required", "Approve Device and Location  Verification Code: 5964505250  © 2026 Keeper Security, Inc. 311 W Monroe St., Suite 406, Chicago IL 60606 USA", "5964505250"),
            ("Your request to reset password", "We received a request to reset your password. Your Reset Code: 724299 If you didn't make this request", "724299"),
        ];
        for (subject, body, want) in cases {
            assert_eq!(verification_code(subject, body), want, "{subject}");
        }
    }

    #[test]
    fn numbers_beside_the_word_code_that_are_not_sign_in_codes_are_ignored() {
        let cases = [
            (
                "Your flight confirmation code",
                "Confirmation code: 482910. Departs 2026-10-04.",
            ),
            (
                "Save 20% with your promo code",
                "Use promo code 4821 at checkout. Order 55512345.",
            ),
            (
                "This week in Claude Code",
                "The Claude Code team shipped 1200 changes in 2026.",
            ),
            (
                "Verify your email",
                "Verify your email by tapping the button. Valid for 24 hours.",
            ),
            (
                "Your receipt from Kilo Code #2633-0984",
                "Amount paid $1500. Questions? Call 1-800-555-0199.",
            ),
            (
                "Your code review",
                "Your code review on PR #20391 is ready.",
            ),
            (
                "Your login code",
                "Visit us at 311 W Monroe St., Chicago IL 60606.",
            ),
        ];
        for (subject, body) in cases {
            assert_eq!(verification_code(subject, body), "", "{subject}");
        }
    }
    use super::*;

    #[test]
    fn throwaway_domains_are_suspicious_and_real_brands_with_digits_are_not() {
        for bad in ["iemoi.zbc", "yyugv.jzl", "628ydjfg.us", "mail.qzxwvt.com"] {
            assert!(suspicious_domain(bad), "{bad}");
        }
        for good in [
            "gmail.com",
            "1800flowers.com",
            "99designs.com",
            "fb02.freshbooks.com",
            "em-s.dropbox.com",
            "bbc.co.uk",
            "canada.ca",
            "payments.interac.ca",
            "sunnybrook.ca",
            "news.telpark.com",
            "mantisx.com",
            "xn--80ak6aa92e.com",
        ] {
            assert!(!suspicious_domain(good), "{good}");
        }
    }

    #[test]
    fn a_github_notification_says_what_happened_as_well_as_why() {
        let ev = |subject: &str, body: &str| github_event("github.com", subject, body);
        assert_eq!(
            ev("", "@ada requested your review on: org/repo#1 Fix\n").as_deref(),
            Some("review_requested_you")
        );
        assert_eq!(
            ev("", "@ada requested review from @org/team on: org/repo#1\n").as_deref(),
            Some("review_requested_team")
        );
        assert_eq!(
            ev("", "\r\n@ada approved this pull request.\r\n").as_deref(),
            Some("approved")
        );
        assert_eq!(
            ev("", "sonarqubecloud[bot] left a comment (org/repo#1)").as_deref(),
            Some("commented")
        );
        assert_eq!(ev("", "Merged #12 into main.").as_deref(), Some("merged"));
        assert_eq!(
            ev("[me/app] Run failed: CI - main (abc)", "").as_deref(),
            Some("run_failed")
        );
        assert_eq!(
            ev("", "## Summary\nAdds a thing.\nYou can view, comment on, or merge this pull request online at:\n").as_deref(),
            Some("opened")
        );
        assert_eq!(
            github_event("example.com", "Run failed: x", "Merged #1"),
            None
        );
    }

    #[test]
    fn a_list_id_is_the_identifier_in_angle_brackets() {
        let mut h = riddle_core::MessageHeaders::default();
        assert_eq!(list_id(&h), "");
        h.list_id = Some("northwind/platform <Platform.northwind.github.com>".into());
        assert_eq!(list_id(&h), "platform.northwind.github.com");
        h.list_id = Some("plain.list.example".into());
        assert_eq!(list_id(&h), "plain.list.example");
    }

    #[test]
    fn free_mailboxes_are_recognised_and_isp_or_company_domains_are_not() {
        for free in [
            "gmail.com",
            "GMAIL.com",
            "yahoo.co.uk",
            "hotmail.fr",
            "outlook.com",
            "icloud.com",
        ] {
            assert!(free_mail_domain(free), "{free}");
        }
        for not_free in [
            "example.com",
            "outlook.office365.com",
            "live.nation.com",
            "mail.google.com",
            "yahooinc.com",
            "bell.net",
            "rogers.com",
        ] {
            assert!(!free_mail_domain(not_free), "{not_free}");
        }
    }

    #[test]
    fn letters_disguised_to_pass_a_filter_are_caught_and_ordinary_prose_is_not() {
        assert!(glyph_substitution(
            "Thank you, and | look forward to your response."
        ));
        assert!(glyph_substitution("Hello, l am writing about your account"));
        assert!(glyph_substitution("Your P\u{0430}yPal account")); // Cyrillic a
        assert!(!glyph_substitution("I look forward to it. 2 l of milk."));
        assert!(!glyph_substitution("Jane Doe | Director | Example Inc"));
        assert!(!glyph_substitution(
            "\u{200C}\u{00A0}\u{200C} preheader padding"
        ));
        assert!(!glyph_substitution("Привет, как дела?"));
    }

    fn facts_for(from: &str, labels: &[&str], dmarc: AuthVerdict, me: bool) -> Facts {
        let mut m = riddle_mail::fixture::blank_message("d", "p");
        m.from = riddle_core::Address::new(from, None);
        m.labels = labels.iter().map(|l| l.to_string()).collect();
        m.is_sent = labels.contains(&"SENT");
        m.headers.authentication.dmarc = dmarc;
        let mut f = Facts {
            first_time_sender: true,
            thread_length: 1,
            ..Default::default()
        };
        refresh_from_message(
            &mut f,
            &m,
            Derived {
                me,
                ..Default::default()
            },
        );
        f
    }

    #[test]
    fn your_own_drafts_are_yours_and_a_forged_own_address_is_not() {
        let draft = facts_for("me@gmail.com", &["DRAFT"], AuthVerdict::None, true);
        assert!(draft.is_draft && draft.from_me && !draft.first_time_sender);
        assert!(!draft.cold_outreach(), "you are not a stranger to yourself");
        let forged = facts_for("me@gmail.com", &["INBOX"], AuthVerdict::Fail, true);
        assert!(!forged.from_me && forged.spoofs_me);
        let unverified = facts_for("me@gmail.com", &["INBOX"], AuthVerdict::None, true);
        assert!(!unverified.spoofs_me, "no verdict is not a failed one");
    }

    #[test]
    fn a_stranger_writing_one_to_one_from_a_free_mailbox_is_cold_outreach() {
        let cold = facts_for("someone96@gmail.com", &["INBOX"], AuthVerdict::Pass, false);
        assert!(cold.free_mail_sender && cold.cold_outreach());
        let mut contact = cold.clone();
        contact.sender_in_contacts = true;
        assert!(!contact.cold_outreach());
        let mut list = cold.clone();
        list.list_unsubscribe = true;
        assert!(!list.cold_outreach());
        let mut answered = cold.clone();
        answered.thread_length = 2;
        assert!(!answered.cold_outreach());
        let mut reply = cold.clone();
        reply.is_reply = true;
        assert!(
            !reply.cold_outreach(),
            "a shop answering a website form is a reply, not an opener"
        );
        let company = facts_for("jobs@example.com", &["INBOX"], AuthVerdict::Pass, false);
        assert!(!company.cold_outreach());
    }

    #[test]
    fn government_domains_are_whole_labels_not_lookalike_suffixes() {
        for gov in [
            "cra-arc.gc.ca",
            "rcmp-grc.gc.ca",
            "notification.canada.ca",
            "canada.ca",
            "ontario.ca",
            "login.gov",
            "irs.gov",
            "gov.ee",
            "email.gov.uk",
            "impots.gouv.fr",
        ] {
            assert!(government_domain(gov), "{gov}");
        }
        for not_gov in [
            "email.petro-canada.ca",
            "canada.ca.refund-now.com",
            "cra-arc-gc.ca",
            "govdeals.com",
            "gov.company.com",
            "transactions.canadapost-postescanada.ca",
        ] {
            assert!(!government_domain(not_gov), "{not_gov}");
        }
    }

    #[test]
    fn the_github_reason_comes_from_the_header_or_the_footer_of_older_mail() {
        let mut headers = riddle_core::MessageHeaders::default();
        assert_eq!(github_reason(&headers, "hello"), None);
        let footer = "subject\nfoo bar\n\nyou are receiving this because your review was requested.\n\nmessage id: <x>";
        assert_eq!(
            github_reason(&headers, footer).as_deref(),
            Some("review_requested")
        );
        assert_eq!(
            github_reason(&headers, "you are receiving this because you commented.").as_deref(),
            Some("comment")
        );
        assert_eq!(
            github_reason(
                &headers,
                "you are receiving this because you are subscribed to this thread."
            )
            .as_deref(),
            Some("subscribed")
        );
        assert_eq!(
            github_reason(
                &headers,
                "you are receiving this because you were mentioned."
            )
            .as_deref(),
            Some("mention")
        );
        assert_eq!(
            github_reason(&headers, "you are receiving this because of the weather.").as_deref(),
            None
        );
        headers
            .extra
            .insert("x-github-reason".into(), "Assign".into());
        assert_eq!(
            github_reason(&headers, footer).as_deref(),
            Some("assign"),
            "the header wins"
        );
    }

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
        // Brands' own other domains, and a brand inside a longer word.
        assert!(!is_lookalike("costalerts.amazonaws.com", &own));
        assert!(!is_lookalike("msg.amazonmusic.com", &own));
        assert!(!is_lookalike("stripedseed.ca", &own));
        assert!(is_lookalike("secure-amazon.com", &own));
        assert!(is_lookalike("amazon-billing.net", &own));
    }

    #[test]
    fn community_posts_are_told_from_the_platforms_own_mail() {
        let c = |sender: &str, name: &str, list: &str| {
            let domain = sender.rsplit('@').next().unwrap();
            community_platform(sender, name, domain, list)
        };
        assert_eq!(c("no-reply@is.ca.nextdoor.com", "Nextdoor", ""), "nextdoor");
        assert_eq!(c("reply@rs.ca.nextdoor.com", "", ""), "nextdoor");
        assert_eq!(c("no-reply@ms.email.nextdoor.com", "Nextdoor", ""), "");
        assert_eq!(
            c("notification@facebookmail.com", "Lakeview Residents", ""),
            "facebook_group"
        );
        assert_eq!(c("notification@facebookmail.com", "Facebook", ""), "");
        assert_eq!(
            c("someone@gmail.com", "", "hoa.googlegroups.com"),
            "google_group"
        );
        assert_eq!(c("info@meetup.com", "Meetup", ""), "meetup");
        assert_eq!(c("garandthumb@creator.patreon.com", "", ""), "creator");
        assert_eq!(c("no-reply@updates.patreon.com", "", ""), "");
        assert_eq!(c("meetup@taxamo.com", "", ""), "");
    }

    #[test]
    fn a_payment_that_did_not_go_through_is_told_from_one_that_did() {
        let failed = [
            (
                "CA$76.84 payment to IcePanel Technologies Inc. was unsuccessful",
                "",
            ),
            ("$5.65 payment to Eleven Labs Inc. was unsuccessful", ""),
            (
                "Payment failure: Google Workspace Business Starter for mdjp.net",
                "",
            ),
            ("Google Workspace: Payment was declined for mdjp.net", ""),
            (
                "Matthew, add a different payment method",
                "Your Uber One payment didn't go through.",
            ),
            (
                "Action needed on your payment",
                "We were unable to process your payment for October.",
            ),
        ];
        for (subject, body) in failed {
            assert!(payment_failed(subject, body), "{subject}");
        }
        let fine = [
            (
                "Payment attempt successful on backup payment method",
                "Your primary payment method failed, so we charged your backup payment method.",
            ),
            (
                "Your receipt from Kilo Code #2633-0984",
                "Payment failed? No: paid in full.",
            ),
            ("Your bill is ready", "Pay by October 20."),
        ];
        for (subject, body) in fine {
            assert!(!payment_failed(subject, body), "{subject}");
        }
        assert!(payment_recovered(fine[0].0, fine[0].1));
        assert!(!payment_recovered(failed[0].0, failed[0].1));
    }
}
