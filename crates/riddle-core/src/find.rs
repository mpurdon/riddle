//! Finds: first-class entities derived from one or more messages (§41).

use crate::ids::{ClassificationId, DigId, FindId, MeshId, MessageId, ThreadId, TraceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FindType {
    Respond,
    Decide,
    Review,
    Do,
    Pay,
    Attend,
    FollowUp,
    Know,
}

impl FindType {
    pub const ALL: [FindType; 8] = [
        FindType::Respond,
        FindType::Decide,
        FindType::Review,
        FindType::Do,
        FindType::Pay,
        FindType::Attend,
        FindType::FollowUp,
        FindType::Know,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            FindType::Respond => "respond",
            FindType::Decide => "decide",
            FindType::Review => "review",
            FindType::Do => "do",
            FindType::Pay => "pay",
            FindType::Attend => "attend",
            FindType::FollowUp => "follow_up",
            FindType::Know => "know",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|t| t.as_str().eq_ignore_ascii_case(s) || t.label().eq_ignore_ascii_case(s))
    }
    pub fn label(self) -> &'static str {
        match self {
            FindType::Respond => "Respond",
            FindType::Decide => "Decide",
            FindType::Review => "Review",
            FindType::Do => "Do",
            FindType::Pay => "Pay",
            FindType::Attend => "Attend",
            FindType::FollowUp => "Follow Up",
            FindType::Know => "Know",
        }
    }
    /// Dated Find types expire after their date.
    pub fn is_dated(self) -> bool {
        matches!(self, FindType::Attend | FindType::Pay)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FindStatus {
    Open,
    Snoozed,
    Resolved,
    Dismissed,
    Expired,
    Superseded,
}

impl FindStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            FindStatus::Open => "open",
            FindStatus::Snoozed => "snoozed",
            FindStatus::Resolved => "resolved",
            FindStatus::Dismissed => "dismissed",
            FindStatus::Expired => "expired",
            FindStatus::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ResolvedBy {
    User,
    ThreadState,
    Temporal,
    Director,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FindSource {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub dig_id: DigId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Find {
    pub id: FindId,
    pub find_type: FindType,
    pub title: String,
    pub summary: String,
    /// 0 lowest .. 100 highest; only Finds above the notify threshold interrupt.
    pub priority: u8,
    pub status: FindStatus,
    pub due_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<ResolvedBy>,
    pub sources: Vec<FindSource>,
    /// Dig ids the sources span; the same thread in two Digs is one Find.
    pub dig_ids: Vec<DigId>,
    /// Cross-Dig deduplication key (normally the RFC Message-ID root of the thread).
    pub identity_key: String,
    pub reason: String,
    pub confidence: Option<f64>,
    pub classification_id: Option<ClassificationId>,
    pub trace_id: Option<TraceId>,
    pub superseded_by: Option<FindId>,
    /// Set when the route that raised it says `+ Replaces`.
    #[serde(default)]
    #[ts(optional)]
    pub series: Option<FindSeries>,
    /// When Temporal Sift closes it as Expired, whatever its type; set by
    /// a route's `+ Expires:`. Unlike `due_at` it is not a deadline: it
    /// neither raises priority nor escalates.
    #[serde(default)]
    #[ts(optional)]
    pub expires_at: Option<DateTime<Utc>>,
    /// When the user undid a rule's closing of it. An undo is a choice to
    /// keep this Find, so from then on no rule closes it again: not a newer
    /// one of its series, not its `Expires:`, not a passed Attend or Pay
    /// date. Without this, undoing an expiry lasted until the next Temporal
    /// pass, a quarter of an hour.
    #[serde(default)]
    #[ts(optional)]
    pub kept_open_at: Option<DateTime<Utc>>,
    /// Raised by a route that said `+ Notify`: it interrupts at once, in any
    /// notification mode. The flag used to be recorded on the decision and
    /// read by nothing, so "tell me now" rules told no one.
    #[serde(default)]
    pub notify: bool,
    /// A one-time sign-in code the message carries (`verification_code`),
    /// so the Find can show it with a copy button; it lapses with the Find.
    #[serde(default)]
    #[ts(optional)]
    pub code: Option<String>,
}

/// A run of Finds where only the newest matters: "Time to write" every
/// morning. One Mesh and one sender make a series; the Find from the newest
/// message stays open and the others are Superseded by it. Ordering by the
/// message's own date rather than by when the Find was made matters because
/// applying a Mesh to history re-decides old mail after new mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FindSeries {
    pub key: String,
    pub message_at: DateTime<Utc>,
}

impl FindSeries {
    pub fn new(mesh: &MeshId, sender: &str, message_at: DateTime<Utc>) -> Self {
        Self {
            key: series_key(mesh, sender),
            message_at,
        }
    }
}

/// A `+ Replaces` series: one Mesh, one sender. Finds and messages share it.
pub fn series_key(mesh: &MeshId, sender: &str) -> String {
    format!("series:{mesh}:{}", sender.trim().to_ascii_lowercase())
}

/// The series a route's `+ Replaces` puts a message in: the sender's, or
/// with `Replaces: same subject` the sender's and the subject's, read past
/// its numbers and reply prefixes, so "example.com expires in 28 days" and
/// "… in 14 days" are one series and two domains are two.
pub fn series_key_scoped(
    mesh: &MeshId,
    sender: &str,
    scope: crate::mesh::SeriesScope,
    subject: &str,
) -> String {
    match scope {
        crate::mesh::SeriesScope::Sender => series_key(mesh, sender),
        crate::mesh::SeriesScope::Subject => {
            format!("{}:{}", series_key(mesh, sender), series_subject(subject))
        }
    }
}

/// A subject as a series name: lower case, reply and forward prefixes
/// dropped, every run of digits one `#`, runs of spaces one space. A
/// reminder counts down ("in 28 days", "in 14 days") and a login notice
/// names an address; neither makes a new series.
pub fn series_subject(subject: &str) -> String {
    let mut s = subject.trim().to_lowercase();
    loop {
        let t = s.trim_start();
        match ["re:", "fwd:", "fw:"]
            .iter()
            .find_map(|p| t.strip_prefix(p))
        {
            Some(rest) => s = rest.trim_start().to_string(),
            None => break,
        }
    }
    let mut out = String::with_capacity(s.len());
    let (mut in_digits, mut in_space) = (false, false);
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
                in_digits = true;
            }
            in_space = false;
            continue;
        }
        in_digits = false;
        if c.is_whitespace() {
            if !in_space && !out.is_empty() {
                out.push(' ');
                in_space = true;
            }
            continue;
        }
        in_space = false;
        out.push(c);
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod series_tests {
    use super::*;
    use crate::mesh::SeriesScope;

    #[test]
    fn a_subject_series_reads_past_numbers_and_reply_prefixes() {
        assert_eq!(
            series_subject("Re: FWD: nasghoul.com Expires in 28 days"),
            "nasghoul.com expires in # days"
        );
        assert_eq!(
            series_subject("nasghoul.com expires in 14 days"),
            series_subject("nasghoul.com  Expires in 28 days")
        );
        assert_ne!(
            series_subject("nasghoul.com expires in 14 days"),
            series_subject("iprm.net expires in 14 days")
        );
        assert_eq!(
            series_subject("Login notification from 10.0.0.1 (Canada (CA))"),
            "login notification from #.#.#.# (canada (ca))"
        );
    }

    #[test]
    fn a_sender_series_ignores_the_subject_and_a_subject_series_does_not() {
        let mesh = MeshId::from("mesh_renewals");
        let by_sender = |subject: &str| {
            series_key_scoped(
                &mesh,
                "Getsupport@moniker.com ",
                SeriesScope::Sender,
                subject,
            )
        };
        assert_eq!(
            by_sender("a.com expires in 28 days"),
            by_sender("b.com expires in 28 days")
        );
        assert_eq!(
            by_sender("anything"),
            series_key(&mesh, "getsupport@moniker.com")
        );
        let by_subject = |subject: &str| {
            series_key_scoped(
                &mesh,
                "getsupport@moniker.com",
                SeriesScope::Subject,
                subject,
            )
        };
        assert_eq!(
            by_subject("a.com expires in 28 days"),
            by_subject("Re: a.com expires in 14 days")
        );
        assert_ne!(
            by_subject("a.com expires in 28 days"),
            by_subject("b.com expires in 28 days")
        );
    }
}

/// What Sift proposes when a message deserves attention (§41). `riddle-finds`
/// joins it to an open Find with the same `identity_key` or creates one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FindProposal {
    pub find_type: FindType,
    pub title: String,
    pub summary: String,
    pub priority: u8,
    pub reason: String,
    /// Cross-Dig deduplication key: the RFC Message-ID root of the thread when
    /// present, else the provider thread id.
    pub identity_key: String,
    pub due_at: Option<DateTime<Utc>>,
    /// Raised snoozed until then: a booking's Find waits for its first day.
    #[serde(default)]
    pub snoozed_until: Option<DateTime<Utc>>,
    pub source: FindSource,
    pub confidence: Option<f64>,
    pub classification_id: Option<ClassificationId>,
    pub trace_id: Option<TraceId>,
    pub series: Option<FindSeries>,
    pub expires_at: Option<DateTime<Utc>>,
    /// The route said `+ Notify`.
    pub notify: bool,
    /// The message's one-time code, when it carries one.
    pub code: Option<String>,
}
