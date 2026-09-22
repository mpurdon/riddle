//! Thread State and Context slices (`ARCHITECTURE.md` §2, §44).
//!
//! Thread State answers "who spoke last, what is open, what did I promise"
//! from the messages alone, recomputed whenever the thread changes from any
//! source (a reply sent from the phone counts). Asks and commitments are
//! extracted by sentence heuristics over the normalized text, deliberately
//! simple: an open ask is a question or request in their latest turn that I
//! have not answered; a commitment is a first-person promise in mine. The
//! judge sees these as text, so they are kept short and bounded.
//!
//! Context slices are retrieved, not stuffed (§44): each named slice is a
//! bounded string built from the store on demand.

use chrono::{DateTime, Utc};
use regex::Regex;
use riddle_core::{DigId, Message, Speaker, ThreadId, ThreadState, UserProfile};
use riddle_errors::Result;
use riddle_mail::normalize::{normalize_message, strip_quoted_reply, strip_signature};
use riddle_store::{repo, Store};
use std::sync::OnceLock;

pub const MAX_ASKS: usize = 5;
pub const MAX_SENTENCE_CHARS: usize = 200;
pub const MAX_SLICE_CHARS: usize = 800;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static regex"))
}

/// Split normalized text into sentences, stripping quotes and signatures first.
pub fn sentences(text: &str) -> Vec<String> {
    let (unquoted, _) = strip_quoted_reply(text);
    let (body, _) = strip_signature(&unquoted);
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    let split = re(&SPLIT, r"(?s)[^.!?\n]+[.!?]?");
    split
        .find_iter(&body)
        .map(|m| m.as_str().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| s.len() > 3)
        .collect()
}

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_SENTENCE_CHARS {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(MAX_SENTENCE_CHARS - 1).collect();
        out.push('…');
        out
    }
}

/// Questions and requests: ends in `?`, or starts with a request phrase.
pub fn is_ask(sentence: &str) -> bool {
    static REQUEST: OnceLock<Regex> = OnceLock::new();
    let request = re(
        &REQUEST,
        r"(?i)^(can|could|would|will|are|is|do|does|did|have|has|should|any chance|please|let me know|kindly|would you mind|when can|what time|which)\b",
    );
    let s = sentence.trim();
    if s.ends_with('?') {
        // Rhetorical or sign-off questions carry no ask.
        return !matches!(
            s.to_ascii_lowercase().as_str(),
            "how are you?" | "hope you're well?" | "right?" | "ok?" | "okay?"
        );
    }
    request.is_match(s) && s.split_whitespace().count() >= 4
}

/// First-person promises: "I'll send…", "I will…", "I can have … by Friday".
pub fn is_commitment(sentence: &str) -> bool {
    static PROMISE: OnceLock<Regex> = OnceLock::new();
    let promise = re(
        &PROMISE,
        r"(?i)\b(i will|i'll|i can (have|send|get|do|make|share|finish|review|look)|i'm going to|i am going to|i shall|we will|we'll|let me (send|get|check|look|find)|i promise|will (send|get|have|do|share|finish|review) (you|it|this|that|them) .*\bby\b)\b",
    );
    static SIGNAL: OnceLock<Regex> = OnceLock::new();
    let signal = re(
        &SIGNAL,
        r"(?i)\b(by (monday|tuesday|wednesday|thursday|friday|saturday|sunday|tomorrow|tonight|end of (the )?(day|week|month)|eod|eow|next week|\d{1,2}(st|nd|rd|th)?( of)? [a-z]+)|(send|get|have|share|finish|review|write|prepare|draft) (you|it|this|that|them|the))\b",
    );
    let s = sentence.trim();
    if s.ends_with('?') {
        return false;
    }
    promise.is_match(s) && signal.is_match(s)
}

fn is_me(m: &Message, profile: &UserProfile) -> bool {
    m.is_sent || profile.is_me(&m.from.email)
}

fn text_of(m: &Message) -> String {
    normalize_message(m).text
}

/// Compute Thread State from a thread's messages (any order).
pub fn compute_thread_state(
    messages: &[Message],
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> ThreadState {
    let mut msgs: Vec<&Message> = messages.iter().collect();
    msgs.sort_by_key(|m| m.date);
    let (dig_id, thread_id) = match msgs.first() {
        Some(m) => (m.dig_id.clone(), m.thread_id.clone()),
        None => (DigId::from(""), ThreadId::from("")),
    };
    let last_speaker = match msgs.last() {
        Some(m) if is_me(m, profile) => Speaker::Me,
        Some(_) => Speaker::Them,
        None => Speaker::Nobody,
    };
    let last_mine_idx = msgs.iter().rposition(|m| is_me(m, profile));
    let last_theirs_idx = msgs.iter().rposition(|m| !is_me(m, profile));
    // A reply by me is a message of mine that follows one of theirs.
    let last_reply_by_me_at = msgs
        .iter()
        .enumerate()
        .filter(|(i, m)| is_me(m, profile) && msgs[..*i].iter().any(|p| !is_me(p, profile)))
        .map(|(_, m)| m.date)
        .max();

    // Their open asks: questions in their messages after my last message.
    let mut open_asks_from_them = Vec::new();
    let their_recent = msgs
        .iter()
        .enumerate()
        .filter(|(i, m)| !is_me(m, profile) && last_mine_idx.is_none_or(|k| *i > k));
    for (_, m) in their_recent {
        for s in sentences(&text_of(m)) {
            if is_ask(&s) && open_asks_from_them.len() < MAX_ASKS {
                open_asks_from_them.push(clip(&s));
            }
        }
    }
    // My open asks: questions in my messages with no later message from them.
    let mut open_asks_from_me = Vec::new();
    let my_recent = msgs
        .iter()
        .enumerate()
        .filter(|(i, m)| is_me(m, profile) && last_theirs_idx.is_none_or(|k| *i > k));
    for (_, m) in my_recent {
        for s in sentences(&text_of(m)) {
            if is_ask(&s) && open_asks_from_me.len() < MAX_ASKS {
                open_asks_from_me.push(clip(&s));
            }
        }
    }
    // Commitments: promises in any of my messages, most recent first.
    let mut my_commitments = Vec::new();
    for m in msgs.iter().rev().filter(|m| is_me(m, profile)) {
        for s in sentences(&text_of(m)) {
            if is_commitment(&s) && my_commitments.len() < MAX_ASKS {
                my_commitments.push(clip(&s));
            }
        }
    }
    ThreadState {
        thread_id,
        dig_id,
        last_speaker,
        open_asks_from_them,
        open_asks_from_me,
        my_commitments,
        last_reply_by_me_at,
        last_message_at: msgs.last().map(|m| m.date),
        message_count: msgs.len() as u32,
        updated_at: now,
    }
}

/// Recompute and store the Thread State for one thread.
pub fn update_thread_state(
    store: &Store,
    dig_id: &DigId,
    thread_id: &ThreadId,
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<ThreadState> {
    let messages = repo::messages::thread_messages(store, dig_id, thread_id)?;
    let mut state = compute_thread_state(&messages, profile, now);
    if messages.is_empty() {
        state.thread_id = thread_id.clone();
        state.dig_id = dig_id.clone();
    }
    repo::thread_state::put(store, &state)?;
    Ok(state)
}

/// Recompute several threads; errors on one thread do not stop the others.
pub fn update_thread_states(
    store: &Store,
    dig_id: &DigId,
    threads: &[ThreadId],
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<usize> {
    let mut n = 0;
    for t in threads {
        update_thread_state(store, dig_id, t, profile, now)?;
        n += 1;
    }
    Ok(n)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSlice {
    pub name: String,
    pub text: String,
}

pub const SLICE_NAMES: &[&str] = &[
    "sender_relationship",
    "thread_state",
    "previous_classifications",
    "recent_corrections",
];

fn bound(s: String) -> String {
    if s.chars().count() <= MAX_SLICE_CHARS {
        s
    } else {
        let mut out: String = s.chars().take(MAX_SLICE_CHARS - 1).collect();
        out.push('…');
        out
    }
}

/// The named Context slices for a message, each bounded in size. Missing
/// data yields a short "unknown" sentence rather than an absent slice, so
/// the judge is told what is not known.
pub fn slices(
    index: &Store,
    dig_store: &Store,
    message: &Message,
    names: &[&str],
) -> Result<Vec<ContextSlice>> {
    let mut out = Vec::new();
    for name in names {
        let text = match *name {
            "sender_relationship" => sender_relationship(index, message)?,
            "thread_state" => thread_state_slice(dig_store, message)?,
            "previous_classifications" => previous_classifications(dig_store, message)?,
            "recent_corrections" => recent_corrections(dig_store, message)?,
            _ => continue,
        };
        out.push(ContextSlice {
            name: name.to_string(),
            text: bound(text),
        });
    }
    Ok(out)
}

fn sender_relationship(index: &Store, m: &Message) -> Result<String> {
    Ok(match repo::people::by_email(index, &m.from.email)? {
        Some(p) => {
            let mut s = format!(
                "Sender {}: {} messages from them, {} from you to them; {}.",
                p.name.clone().unwrap_or_else(|| m.from.email.clone()),
                p.messages_from,
                p.messages_to,
                if p.replied_to_by_user {
                    "you have replied to them before"
                } else {
                    "you have never replied to them"
                }
            );
            if let Some(first) = p.first_contact_at {
                s.push_str(&format!(" First contact {}.", first.format("%Y-%m-%d")));
            }
            s.push_str(&format!(
                " Relationship strength {:.2}.",
                p.relationship_strength
            ));
            if p.automated {
                s.push_str(" Automated sender.");
            }
            if p.mailing_list {
                s.push_str(" Mailing list.");
            }
            if p.auth_failures > 0 {
                s.push_str(&format!(
                    " {} messages failed authentication.",
                    p.auth_failures
                ));
            }
            if p.in_contacts {
                s.push_str(" In your contacts.");
            }
            s
        }
        None => "Sender is unknown: no previous mail from or to them.".into(),
    })
}

fn thread_state_slice(store: &Store, m: &Message) -> Result<String> {
    Ok(match repo::thread_state::get(store, &m.thread_id)? {
        Some(t) => t.summary(),
        None => "Thread State unknown: first message of a new thread.".into(),
    })
}

fn previous_classifications(store: &Store, m: &Message) -> Result<String> {
    let mut lines = Vec::new();
    let history = repo::classifications::history(store, &m.id)?;
    for k in history.iter().filter(|k| !k.shadow).take(3) {
        lines.push(format!(
            "This message was {} by {:?}{}.",
            k.disposition.kind().as_str(),
            k.mechanism,
            if k.superseded_at.is_some() {
                " (superseded)"
            } else {
                ""
            }
        ));
    }
    let siblings = repo::messages::thread_messages(store, &m.dig_id, &m.thread_id)?;
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for s in siblings.iter().filter(|s| s.id != m.id) {
        if let Some(k) = repo::classifications::current(store, &s.id)? {
            *counts
                .entry(k.disposition.kind().as_str().to_string())
                .or_default() += 1;
        }
    }
    if !counts.is_empty() {
        lines.push(format!(
            "Earlier in this thread: {}.",
            counts
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if lines.is_empty() {
        lines.push("No previous classification for this message or its thread.".into());
    }
    Ok(lines.join(" "))
}

fn recent_corrections(store: &Store, m: &Message) -> Result<String> {
    let mut lines = Vec::new();
    for k in repo::corrections::active(store, 50)? {
        if lines.len() >= 5 {
            break;
        }
        let same_thread_or_sender = if k.message_id == m.id {
            true
        } else {
            match repo::messages::get(store, &m.dig_id, &k.message_id)? {
                Some(other) => other.thread_id == m.thread_id || other.from.email == m.from.email,
                None => false,
            }
        };
        if !same_thread_or_sender {
            continue;
        }
        let what = match (&k.from_disposition, &k.to_disposition) {
            (Some(f), Some(t)) => format!(
                "{:?} → {} to {}",
                k.kind,
                f.kind().as_str(),
                t.kind().as_str()
            ),
            _ => format!("{:?}", k.kind),
        };
        lines.push(format!(
            "You corrected mail from this sender or thread on {}: {}.",
            k.created_at.format("%Y-%m-%d"),
            what
        ));
    }
    if lines.is_empty() {
        lines.push("No corrections involving this sender or thread.".into());
    }
    Ok(lines.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::Address;
    use riddle_mail::fixture::blank_message;

    fn profile() -> UserProfile {
        UserProfile {
            addresses: vec!["matt@example.com".into()],
            ..Default::default()
        }
    }

    fn msg(id: &str, from: &str, sent: bool, text: &str, hours: i64) -> Message {
        let mut m = blank_message("d1", id);
        m.thread_id = ThreadId::from("d1:t1");
        m.from = Address::new(from, None);
        m.is_sent = sent;
        m.body_text = Some(text.into());
        m.date = Utc::now() - chrono::Duration::hours(48 - hours);
        m
    }

    #[test]
    fn an_unanswered_ask_from_me_is_open_and_i_spoke_last() {
        let msgs = vec![msg(
            "1",
            "matt@example.com",
            true,
            "Hi Priya,\n\nCan you send me the updated numbers for the board deck?\n\nThanks,\nMatt",
            0,
        )];
        let t = compute_thread_state(&msgs, &profile(), Utc::now());
        assert_eq!(t.last_speaker, Speaker::Me);
        assert_eq!(
            t.open_asks_from_me,
            vec!["Can you send me the updated numbers for the board deck?".to_string()]
        );
        assert!(t.open_asks_from_them.is_empty());
        assert!(
            t.last_reply_by_me_at.is_none(),
            "starting a thread is not a reply"
        );
        assert_eq!(t.message_count, 1);
    }

    #[test]
    fn their_question_is_open_until_i_reply_and_my_promise_is_a_commitment() {
        let theirs = msg("1", "priya@example.com", false, "Hi Matt,\n\nWhere are we on the numbers? Could you send them before Thursday?\n\nPriya", 0);
        let t = compute_thread_state(std::slice::from_ref(&theirs), &profile(), Utc::now());
        assert_eq!(t.last_speaker, Speaker::Them);
        assert_eq!(t.open_asks_from_them.len(), 2);
        let mine = msg("2", "matt@example.com", true, "I'll get you the numbers by Friday.\n\nOn Tue, Sep 15, 2026 at 3:02 PM Priya <priya@example.com> wrote:\n> Where are we on the numbers?", 2);
        let t = compute_thread_state(&[theirs, mine], &profile(), Utc::now());
        assert_eq!(t.last_speaker, Speaker::Me);
        assert!(t.open_asks_from_them.is_empty(), "answered");
        assert_eq!(
            t.my_commitments,
            vec!["I'll get you the numbers by Friday.".to_string()]
        );
        assert!(t.last_reply_by_me_at.is_some());
        assert!(
            t.open_asks_from_me.is_empty(),
            "quoted questions are not my asks"
        );
    }

    #[test]
    fn asks_and_commitments_are_recognised_by_shape() {
        assert!(is_ask("Can you send me the deck?"));
        assert!(is_ask("Please review the attached document by Monday"));
        assert!(!is_ask("How are you?"));
        assert!(!is_ask("Thanks."));
        assert!(is_commitment(
            "I will send the migration plan by end of day tomorrow."
        ));
        assert!(is_commitment("I can have the write-up to you by Thursday."));
        assert!(!is_commitment("Will you send it by Friday?"));
        assert!(!is_commitment("I will be out on Friday."));
    }

    #[test]
    fn thread_state_is_stored_and_slices_are_bounded() {
        let stores = riddle_store::Stores::in_memory().unwrap();
        let dig = DigId::from("d1");
        let store = stores.dig(&dig).unwrap();
        let theirs = msg(
            "1",
            "priya@example.com",
            false,
            "Could you approve the vendor contract today?",
            0,
        );
        repo::messages::upsert(&store, &theirs).unwrap();
        let t =
            update_thread_state(&store, &dig, &theirs.thread_id, &profile(), Utc::now()).unwrap();
        assert_eq!(t.open_asks_from_them.len(), 1);
        assert!(repo::thread_state::get(&store, &theirs.thread_id)
            .unwrap()
            .is_some());
        let s = slices(&stores.index(), &store, &theirs, SLICE_NAMES).unwrap();
        assert_eq!(s.len(), 4);
        assert!(s.iter().all(|x| x.text.chars().count() <= MAX_SLICE_CHARS));
        assert!(s[0].text.contains("unknown"));
        assert!(s[1].text.contains("Open asks from them"));
        assert!(s[2].text.contains("No previous classification"));
        assert!(s[3].text.contains("No corrections"));
    }
}
