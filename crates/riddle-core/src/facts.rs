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

impl Facts {
    /// Look a fact up by its Mesh name. Booleans are 1/0, numbers as-is,
    /// strings as strings; unknown names are `None` so a Mesh referencing a
    /// fact that does not exist fails validation rather than silently
    /// evaluating false.
    pub fn get(&self, name: &str) -> Option<FactValue> {
        use FactValue::*;
        Some(match name {
            "sender_domain" => Text(self.sender_domain.clone()),
            "sender_in_contacts" => Bool(self.sender_in_contacts),
            "sender_replied_to_before" => Bool(self.sender_replied_to_before),
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
            "deadline_days" => Number(self.deadlines.iter().map(|d| d.days_from_now).min().unwrap_or(i64::MAX) as f64),
            "is_sent" => Bool(self.is_sent),
            "lookalike_domain" => Bool(self.lookalike_domain),
            "sender_in_never_hidden" => Bool(self.sender_in_never_hidden),
            "sender_is_vip" => Bool(self.sender_is_vip),
            "automated_sender" => Bool(self.mailing_list || self.precedence_bulk || self.auto_submitted || self.list_unsubscribe),
            "known_sender" => Bool(self.sender_in_contacts || self.sender_replied_to_before),
            _ => return None,
        })
    }

    pub const NAMES: &'static [&'static str] = &[
        "sender_domain", "sender_in_contacts", "sender_replied_to_before", "first_time_sender",
        "same_organisation", "mailing_list", "list_unsubscribe", "precedence_bulk", "auto_submitted",
        "spf_pass", "dkim_pass", "dmarc_pass", "reply_to_mismatch", "gmail_category", "gmail_important",
        "gmail_spam", "directly_addressed", "cc_only", "thread_length", "has_calendar_part",
        "has_structured_receipt", "has_attachments", "language", "urgency_language_present",
        "asks_for_payment_or_credentials", "has_deadline", "deadline_days", "is_sent", "lookalike_domain",
        "sender_in_never_hidden", "sender_is_vip", "automated_sender", "known_sender",
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum FactValue {
    Bool(bool),
    Number(f64),
    Text(String),
}
