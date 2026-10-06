//! People, Thread State and the User Profile (§2). `personally_relevant` and
//! `directly_addressed` are undefined without the profile.

use crate::ids::{DigId, PersonId, ThreadId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Person {
    pub id: PersonId,
    pub emails: Vec<String>,
    pub name: Option<String>,
    pub organisation: Option<String>,
    /// 0..1, from reply frequency and recency.
    pub relationship_strength: f64,
    pub first_contact_at: Option<DateTime<Utc>>,
    pub last_contact_at: Option<DateTime<Utc>>,
    pub replied_to_by_user: bool,
    pub messages_from: u64,
    pub messages_to: u64,
    pub automated: bool,
    pub mailing_list: bool,
    pub auth_failures: u32,
    pub spam_verdicts: u32,
    pub in_contacts: bool,
    pub dig_ids: Vec<DigId>,
    pub updated_at: DateTime<Utc>,
    /// How often this sender writes, from when their mail arrived.
    #[serde(default)]
    #[ts(optional)]
    pub cadence: Option<Cadence>,
    /// At most about one message a day, nearly always within an hour of
    /// the same local time: mail on a timer, as a reminder is. Cadence says
    /// how often; this tells a scheduled reminder from a sender that is
    /// daily because the user's own actions set it off (purchase
    /// confirmations, notifications).
    #[serde(default)]
    pub on_schedule: bool,
}

/// How regularly a sender writes, classified from arrival dates by
/// `riddle_people::cadence_of` in the user's timezone. A fact, not a
/// judgment: a daily sender is daily whether it sends a journal prompt or a
/// sale, so a Mesh pairs it with a Signal to say which one matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Cadence {
    /// Most days, weekends included.
    Daily,
    /// Most weekdays, never at weekends.
    Weekdays,
    /// Two to four days apart, like a store's sale mail.
    EveryFewDays,
    Weekly,
    Monthly,
    /// Often enough to judge, with no steady rhythm.
    Irregular,
}

impl Cadence {
    pub fn as_str(self) -> &'static str {
        match self {
            Cadence::Daily => "daily",
            Cadence::Weekdays => "weekdays",
            Cadence::EveryFewDays => "every_few_days",
            Cadence::Weekly => "weekly",
            Cadence::Monthly => "monthly",
            Cadence::Irregular => "irregular",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Speaker {
    Me,
    Them,
    #[default]
    Nobody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadState {
    pub thread_id: ThreadId,
    pub dig_id: DigId,
    pub last_speaker: Speaker,
    pub open_asks_from_them: Vec<String>,
    pub open_asks_from_me: Vec<String>,
    pub my_commitments: Vec<String>,
    pub last_reply_by_me_at: Option<DateTime<Utc>>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub message_count: u32,
    pub updated_at: DateTime<Utc>,
}

impl ThreadState {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Thread of {} messages; last speaker: {}.",
            self.message_count,
            match self.last_speaker {
                Speaker::Me => "me",
                Speaker::Them => "them",
                Speaker::Nobody => "nobody",
            }
        );
        if !self.open_asks_from_them.is_empty() {
            s.push_str(&format!(
                " Open asks from them: {}.",
                self.open_asks_from_them.join("; ")
            ));
        }
        if !self.open_asks_from_me.is_empty() {
            s.push_str(&format!(
                " Open asks from me: {}.",
                self.open_asks_from_me.join("; ")
            ));
        }
        if !self.my_commitments.is_empty() {
            s.push_str(&format!(
                " My commitments: {}.",
                self.my_commitments.join("; ")
            ));
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserProfile {
    pub addresses: Vec<String>,
    pub aliases: Vec<String>,
    pub group_addresses: Vec<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub organisation_domains: Vec<String>,
    pub active_projects: Vec<String>,
    pub vips: Vec<String>,
    pub family: Vec<String>,
    /// Addresses and domains (prefix `@`) that can never be Spoil or Contamination.
    pub never_hidden: Vec<String>,
    pub timezone: Option<String>,
}

/// The user's zone for "their day": a profile timezone the tz database
/// knows, else UTC rather than a guess. One rule for every local-day reading
/// (expiry, cadence, schedule), so they put an arrival on the same day.
pub fn user_zone(timezone: Option<&str>) -> chrono_tz::Tz {
    timezone
        .and_then(|t| t.parse().ok())
        .unwrap_or(chrono_tz::Tz::UTC)
}

impl UserProfile {
    pub fn is_me(&self, email: &str) -> bool {
        let e = email.to_lowercase();
        self.addresses
            .iter()
            .chain(self.aliases.iter())
            .any(|a| a.eq_ignore_ascii_case(&e))
    }
    pub fn is_never_hidden(&self, email: &str) -> bool {
        Self::list_matches(&self.never_hidden, email)
    }
    pub fn is_vip(&self, email: &str) -> bool {
        Self::list_matches(&self.vips, email)
    }
    pub fn is_family(&self, email: &str) -> bool {
        Self::list_matches(&self.family, email)
    }
    pub fn is_own_organisation(&self, email: &str) -> bool {
        let domain = email
            .rsplit_once('@')
            .map(|(_, d)| d.to_lowercase())
            .unwrap_or_default();
        self.organisation_domains
            .iter()
            .any(|d| d.eq_ignore_ascii_case(&domain))
    }
    fn list_matches(list: &[String], email: &str) -> bool {
        let e = email.to_lowercase();
        let domain = e.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
        list.iter().any(|entry| {
            let entry = entry.to_lowercase();
            if let Some(d) = entry.strip_prefix('@') {
                d == domain
            } else {
                entry == e
            }
        })
    }
}

/// What Sift learned from a sent message (§17 "Sent mail"): the results feed
/// Thread State, which `riddle-context` recomputes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadStateUpdate {
    pub thread_id: ThreadId,
    pub dig_id: DigId,
    pub message_id: crate::ids::MessageId,
    pub sent_at: DateTime<Utc>,
    pub asks_question: bool,
    pub makes_commitment: bool,
    pub requests_action: bool,
    /// Calibrated scores behind the booleans, for the inspector.
    pub scores: Vec<(String, f64)>,
}
