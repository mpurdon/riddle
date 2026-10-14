//! Deterministic facts: the fixed vocabulary computed without any model
//! (`ARCHITECTURE.md` §17). Meshes may reference these by name; dates and
//! counts are compared here, never asked of the judge (§18).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Facts {
    pub sender_domain: String,
    /// How often the sender writes (`Cadence::as_str`); empty while there
    /// is too little mail from them to say.
    #[serde(default)]
    pub sender_cadence: String,
    /// The sender writes about once a day at the same local time
    /// (`Person::on_schedule`).
    #[serde(default)]
    pub sender_on_schedule: bool,
    /// An official government sender: a government domain (see
    /// `riddle_sift::facts::government_domain`) whose mail passed DMARC, so
    /// the From address is theirs and not a scammer's use of their name.
    #[serde(default)]
    pub government_sender: bool,
    /// The message is the user's own: from one of their addresses, and sent,
    /// a draft, or authenticated. Mail that only claims an own address on
    /// the From line is not; that is a classic scam (see `riddle_sift::gates`).
    #[serde(default)]
    pub from_me: bool,
    /// One of the user's addresses on the From line of mail that is neither
    /// sent nor a draft and failed DMARC outright: "I have access to your
    /// account" sent as if from the user. An absent verdict is not a failure.
    #[serde(default)]
    pub spoofs_me: bool,
    /// The message answers another (`In-Reply-To` or `References`): a
    /// business replying to a website form the user filled in has no thread
    /// in the mailbox, but it is not an opener.
    #[serde(default)]
    pub is_reply: bool,
    /// A Gmail draft; Sift files it without judging it.
    #[serde(default)]
    pub is_draft: bool,
    /// The thread already has mail from the user or from a sender the user
    /// knows. A CPA's reply in a thread the user's household was in was
    /// "unknown" because the user had never written to that address, so it
    /// skipped Human correspondence and ended Unresolved (13 Oct 2026).
    #[serde(default)]
    pub known_thread: bool,
    /// The user knows someone at the sender's domain (a contact, or someone
    /// they replied to), and the domain is one organisation's
    /// (`organisation_domain`): a new person at the user's accounting firm
    /// is not a stranger; a bell.net or zendesk.com sender still is.
    #[serde(default)]
    pub known_domain: bool,
    /// The sender uses a consumer mailbox anyone can open (`gmail.com`,
    /// `outlook.com`, `icloud.com`…; `riddle_sift::facts::free_mail_domain`).
    /// Authentication there proves the account sent it, not who owns it.
    #[serde(default)]
    pub free_mail_sender: bool,
    /// Letters disguised to slip past filters: a pipe or a lone `l`
    /// standing for the pronoun I ("| look forward"), or a word mixing Latin
    /// with Cyrillic letters (`riddle_sift::facts::glyph_substitution`).
    #[serde(default)]
    pub glyph_substitution: bool,
    /// The sender's domain ends in a top-level domain that does not exist
    /// (`.zbc`, `.jzl`) or its name is a random string of letters and
    /// digits (`628ydjfg.us`): throwaway domains phishing runs from
    /// (`riddle_sift::facts::suspicious_domain`).
    #[serde(default)]
    pub suspicious_domain: bool,
    /// The sender's address, lowercase.
    #[serde(default)]
    pub sender: String,
    /// The sender's display name, e.g. `dependabot[bot]`.
    #[serde(default)]
    pub sender_name: String,
    #[serde(default)]
    pub subject: String,
    /// `MessageHeaders::extra`, keyed lowercase; read as `header <Name>`.
    #[serde(default)]
    #[ts(type = "Record<string, string>")]
    pub headers: std::collections::BTreeMap<String, String>,
    pub sender_in_contacts: bool,
    pub sender_replied_to_before: bool,
    pub first_time_sender: bool,
    pub same_organisation: bool,
    pub mailing_list: bool,
    pub list_unsubscribe: bool,
    pub precedence_bulk: bool,
    pub auto_submitted: bool,
    pub spf_pass: bool,
    pub dkim_pass: bool,
    pub dmarc_pass: bool,
    pub reply_to_mismatch: bool,
    /// Gmail category label without the prefix: personal, social, promotions, updates, forums.
    pub gmail_category: Option<String>,
    pub gmail_important: bool,
    pub gmail_spam: bool,
    pub directly_addressed: bool,
    pub cc_only: bool,
    pub thread_length: u32,
    pub my_last_reply_at: Option<chrono::DateTime<chrono::Utc>>,
    pub has_calendar_part: bool,
    pub has_structured_receipt: bool,
    pub has_attachments: bool,
    /// ISO 639-1, or "und".
    pub language: String,
    pub urgency_language_present: bool,
    pub asks_for_payment_or_credentials: bool,
    /// Deadlines parsed from the text, already compared to now.
    pub deadlines: Vec<Deadline>,
    pub is_sent: bool,
    pub lookalike_domain: bool,
    pub sender_in_never_hidden: bool,
    pub sender_is_vip: bool,
    /// Why GitHub sent a notification: `review_requested`, `mention`,
    /// `assign`, `author`, `comment`, `subscribed`, `ci_activity`, `state_change`,
    /// `security_alert`, `team_mention`, `push`, `manual`. From the
    /// `X-GitHub-Reason` header, or the footer of mail synced before that
    /// header was kept. Empty for anything else.
    #[serde(default)]
    pub github_reason: Option<String>,
    /// What happened on GitHub: `review_requested_you`,
    /// `review_requested_team`, `changes_requested`, `approved`,
    /// `commented`, `merged`, `closed`, `pushed`, `run_failed`, `opened`
    /// (`riddle_sift::facts::github_event`). Empty for anything else.
    #[serde(default)]
    pub github_event: Option<String>,
    /// The List-Id identifier, lowercase (`platform.northwind.github.com`);
    /// empty when the mail is not from a list.
    #[serde(default)]
    pub list_id: String,
    /// The one-time sign-in or verification code the message carries
    /// (`riddle_sift::facts::verification_code`); empty when none.
    #[serde(default)]
    pub verification_code: String,
    /// A payment that did not go through: failed, declined, unsuccessful
    /// (`riddle_sift::facts::payment_failed`).
    #[serde(default)]
    pub payment_failed: bool,
    /// A payment that went through on a backup method after the main one
    /// failed (`riddle_sift::facts::payment_recovered`).
    #[serde(default)]
    pub payment_recovered: bool,
    /// The community a post comes from (`nextdoor`, `facebook_group`,
    /// `google_group`, `groups_io`, `meetup`, `creator`), empty otherwise
    /// (`riddle_sift::facts::community_platform`).
    #[serde(default)]
    pub community_platform: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Deadline {
    pub at: chrono::DateTime<chrono::Utc>,
    pub text: String,
    pub days_from_now: i64,
    pub past: bool,
}

/// How a header fact is named inside a Mesh: `header X-GitHub-Reason`
/// parses to the fact `header:x-github-reason`.
pub const HEADER_PREFIX: &str = "header:";

impl Facts {
    /// Look a fact up by its Mesh name. Booleans are 1/0, numbers as-is,
    /// strings as strings; unknown names are `None` so a Mesh referencing a
    /// fact that does not exist fails validation rather than silently
    /// evaluating false.
    pub fn get(&self, name: &str) -> Option<FactValue> {
        use FactValue::*;
        // An absent header is empty text: `=` fails and `!=` holds, rather
        // than the condition being unknown.
        if let Some(h) = name.strip_prefix(HEADER_PREFIX) {
            return Some(Text(self.headers.get(h).cloned().unwrap_or_default()));
        }
        Some(match name {
            "sender_domain" => Text(self.sender_domain.clone()),
            "sender" => Text(self.sender.clone()),
            "sender_name" => Text(self.sender_name.clone()),
            "sender_cadence" => Text(self.sender_cadence.clone()),
            "sender_on_schedule" => Bool(self.sender_on_schedule),
            "government_sender" => Bool(self.government_sender),
            "from_me" => Bool(self.from_me),
            "free_mail_sender" => Bool(self.free_mail_sender),
            "glyph_substitution" => Bool(self.glyph_substitution),
            "cold_outreach" => Bool(self.cold_outreach()),
            "suspicious_domain" => Bool(self.suspicious_domain),
            "subject" => Text(self.subject.clone()),
            "sender_in_contacts" => Bool(self.sender_in_contacts),
            "sender_replied_to_before" => Bool(self.sender_replied_to_before),
            "known_thread" => Bool(self.known_thread),
            "known_domain" => Bool(self.known_domain),
            "first_time_sender" => Bool(self.first_time_sender),
            "same_organisation" => Bool(self.same_organisation),
            "mailing_list" => Bool(self.mailing_list),
            "list_unsubscribe" => Bool(self.list_unsubscribe),
            "precedence_bulk" => Bool(self.precedence_bulk),
            "auto_submitted" => Bool(self.auto_submitted),
            "spf_pass" => Bool(self.spf_pass),
            "dkim_pass" => Bool(self.dkim_pass),
            "dmarc_pass" => Bool(self.dmarc_pass),
            "reply_to_mismatch" => Bool(self.reply_to_mismatch),
            "gmail_category" => Text(self.gmail_category.clone().unwrap_or_default()),
            "gmail_important" => Bool(self.gmail_important),
            "gmail_spam" => Bool(self.gmail_spam),
            "directly_addressed" => Bool(self.directly_addressed),
            "cc_only" => Bool(self.cc_only),
            "thread_length" => Number(self.thread_length as f64),
            "has_calendar_part" => Bool(self.has_calendar_part),
            "has_structured_receipt" => Bool(self.has_structured_receipt),
            "has_attachments" => Bool(self.has_attachments),
            "language" => Text(self.language.clone()),
            "urgency_language_present" => Bool(self.urgency_language_present),
            "asks_for_payment_or_credentials" => Bool(self.asks_for_payment_or_credentials),
            "has_deadline" => Bool(!self.deadlines.is_empty()),
            "deadline_days" => Number(
                self.deadlines
                    .iter()
                    .map(|d| d.days_from_now)
                    .min()
                    .unwrap_or(i64::MAX) as f64,
            ),
            "is_sent" => Bool(self.is_sent),
            "lookalike_domain" => Bool(self.lookalike_domain),
            "sender_in_never_hidden" => Bool(self.sender_in_never_hidden),
            "sender_is_vip" => Bool(self.sender_is_vip),
            "github_reason" => Text(self.github_reason.clone().unwrap_or_default()),
            "github_event" => Text(self.github_event.clone().unwrap_or_default()),
            "list_id" => Text(self.list_id.clone()),
            "verification_code" => Text(self.verification_code.clone()),
            "has_verification_code" => Bool(!self.verification_code.is_empty()),
            "payment_failed" => Bool(self.payment_failed),
            "payment_recovered" => Bool(self.payment_recovered),
            "community_platform" => Text(self.community_platform.clone()),
            // A group the user belongs to; a creator they back is not one.
            "from_community" => {
                Bool(!self.community_platform.is_empty() && self.community_platform != "creator")
            }
            "automated_sender" => Bool(
                self.mailing_list
                    || self.precedence_bulk
                    || self.auto_submitted
                    || self.list_unsubscribe,
            ),
            "known_sender" => Bool(self.known()),
            _ => return None,
        })
    }

    pub const NAMES: &'static [&'static str] = &[
        "sender_domain",
        "sender",
        "sender_name",
        "sender_cadence",
        "sender_on_schedule",
        "government_sender",
        "from_me",
        "free_mail_sender",
        "glyph_substitution",
        "cold_outreach",
        "suspicious_domain",
        "subject",
        "sender_in_contacts",
        "sender_replied_to_before",
        "known_thread",
        "known_domain",
        "first_time_sender",
        "same_organisation",
        "mailing_list",
        "list_unsubscribe",
        "precedence_bulk",
        "auto_submitted",
        "spf_pass",
        "dkim_pass",
        "dmarc_pass",
        "reply_to_mismatch",
        "gmail_category",
        "gmail_important",
        "gmail_spam",
        "directly_addressed",
        "cc_only",
        "thread_length",
        "has_calendar_part",
        "has_structured_receipt",
        "has_attachments",
        "language",
        "urgency_language_present",
        "asks_for_payment_or_credentials",
        "has_deadline",
        "deadline_days",
        "is_sent",
        "lookalike_domain",
        "sender_in_never_hidden",
        "sender_is_vip",
        "github_reason",
        "github_event",
        "list_id",
        "verification_code",
        "has_verification_code",
        "payment_failed",
        "payment_recovered",
        "community_platform",
        "from_community",
        "automated_sender",
        "known_sender",
    ];

    /// The user has a relationship with the sender, or is the sender.
    pub fn known(&self) -> bool {
        self.sender_in_contacts || self.sender_replied_to_before || self.from_me
    }

    /// Unsolicited one-to-one mail from a stranger's free mailbox: nobody the
    /// user knows or protects, a consumer domain, none of the headers bulk
    /// mail carries, not a reply, and a thread the user has not answered. It describes
    /// how the mail arrived, not what it says; scams open this way ("I saw
    /// your comment", a recruiter from a Gmail address), and so do some real
    /// people, which is why a rule pairs it with a Signal.
    pub fn cold_outreach(&self) -> bool {
        self.free_mail_sender
            && !self.known()
            && !self.is_sent
            && !self.same_organisation
            && !self.sender_in_never_hidden
            && !self.sender_is_vip
            && !self.mailing_list
            && !self.list_unsubscribe
            && !self.precedence_bulk
            && !self.auto_submitted
            && !self.is_reply
            && self.my_last_reply_at.is_none()
            && self.thread_length <= 1
    }
}

/// Consumer mailboxes anyone can open in a minute. Exact domains, plus the
/// big providers under any country ending (`yahoo.co.uk`, `hotmail.fr`).
/// ISP mailboxes (`bell.net`, `rogers.com`, `sympatico.ca`) are not here:
/// they come with a paid account tied to a customer, and the first list put
/// a shop's reply from one under cold outreach.
const FREE_MAIL: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "msn.com",
    "ymail.com",
    "rocketmail.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "proton.me",
    "protonmail.com",
    "pm.me",
    "mail.com",
    "zoho.com",
    "tutanota.com",
    "tuta.io",
    "fastmail.com",
    "hushmail.com",
    "mail.ru",
    "qq.com",
    "163.com",
    "126.com",
];

/// Providers whose free mailboxes exist under many country endings.
const FREE_MAIL_BRANDS: &[&str] = &[
    "hotmail", "outlook", "live", "yahoo", "aol", "gmx", "yandex",
];

/// Whether a sender domain is a consumer mailbox (see `FREE_MAIL`); the
/// `free_mail_sender` fact, and never an organisation domain. A brand
/// counts only under a bare public ending (`outlook.com`, `yahoo.co.uk`), so
/// `outlook.office365.com` or `live.nation.com` is not one.
pub fn free_mail_domain(domain: &str) -> bool {
    let d = domain.trim_end_matches('.').to_ascii_lowercase();
    if FREE_MAIL.contains(&d.as_str()) {
        return true;
    }
    let Some((brand, ending)) = d.split_once('.') else {
        return false;
    };
    FREE_MAIL_BRANDS.contains(&brand)
        && ending.split('.').count() <= 2
        && ending.split('.').all(|l| (2..=3).contains(&l.len()))
}

/// Domains many unrelated people or companies send from: ISP mailboxes,
/// which are personal accounts though not free, and shared help desks,
/// which send for thousands of companies (`moniker.zendesk.com`).
const SHARED_DOMAINS: &[&str] = &[
    "bell.net",
    "sympatico.ca",
    "rogers.com",
    "shaw.ca",
    "telus.net",
    "videotron.ca",
    "cogeco.ca",
    "comcast.net",
    "verizon.net",
    "att.net",
    "sbcglobal.net",
    "btinternet.com",
    "orange.fr",
    "zendesk.com",
    "freshdesk.com",
    "helpscout.net",
    "intercom-mail.com",
    "intercom.io",
    "zohodesk.com",
    "salesforce.com",
    "force.com",
    "hubspot.com",
    "mailchimp.com",
];

/// Whether knowing one person at this domain says anything about the next:
/// not for a consumer mailbox, an ISP mailbox or a shared help desk
/// (`Facts::known_domain`). Subdomains of a shared domain count as shared.
pub fn organisation_domain(domain: &str) -> bool {
    let d = domain.trim_end_matches('.').to_ascii_lowercase();
    !d.is_empty()
        && !free_mail_domain(&d)
        && !SHARED_DOMAINS
            .iter()
            .any(|s| d == *s || d.ends_with(&format!(".{s}")))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum FactValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

#[cfg(test)]
mod organisation_domain_tests {
    use super::organisation_domain;

    #[test]
    fn only_one_organisation_s_domain_makes_its_people_known() {
        assert!(organisation_domain("delyaccounting.ca"));
        for shared in [
            "gmail.com",
            "outlook.com",
            "bell.net",
            "moniker.zendesk.com",
            "",
        ] {
            assert!(!organisation_domain(shared), "{shared}");
        }
    }
}
