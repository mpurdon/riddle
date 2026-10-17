//! A suggestion Riddle makes about its own rules, for the Director to voice
//! and the user to take or leave. Nothing here changes a Mesh: a suggestion
//! becomes a proposal with its rule text and a simulation, and the user
//! says yes (§28, §46).

use crate::{FindId, MeshId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What kind of organisation a sender is, judged once per domain from one
/// of its messages, so a new vendor can be offered the list it belongs on
/// before anyone has told Riddle about it (17 Oct 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SenderKind {
    CloudOrHosting,
    DomainRegistrar,
    DeveloperTooling,
    PaymentsService,
    AccountingOrAdviser,
    Retailer,
    Newsletter,
    Government,
    PersonOrOther,
}

impl SenderKind {
    /// Every kind, in the order the judge is offered them.
    pub const ALL: [SenderKind; 9] = [
        SenderKind::CloudOrHosting,
        SenderKind::DomainRegistrar,
        SenderKind::DeveloperTooling,
        SenderKind::PaymentsService,
        SenderKind::AccountingOrAdviser,
        SenderKind::Retailer,
        SenderKind::Newsletter,
        SenderKind::Government,
        SenderKind::PersonOrOther,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SenderKind::CloudOrHosting => "cloud_or_hosting",
            SenderKind::DomainRegistrar => "domain_registrar",
            SenderKind::DeveloperTooling => "developer_tooling",
            SenderKind::PaymentsService => "payments_service",
            SenderKind::AccountingOrAdviser => "accounting_or_adviser",
            SenderKind::Retailer => "retailer",
            SenderKind::Newsletter => "newsletter",
            SenderKind::Government => "government",
            SenderKind::PersonOrOther => "person_or_other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    /// The choice as the judge is given it.
    pub fn definition(self) -> &'static str {
        match self {
            SenderKind::CloudOrHosting => "A cloud, hosting or infrastructure provider: compute, storage, DNS hosting, CDN, servers, usage statements.",
            SenderKind::DomainRegistrar => "A domain name registrar: domain registrations, renewals, expiries, transfers.",
            SenderKind::DeveloperTooling => "A developer tool or service: code hosting, CI, monitoring, design or modelling tools, APIs, developer subscriptions.",
            SenderKind::PaymentsService => "A service that moves money for the recipient: e-transfers, PayPal, Wise, a processor's notice that money was sent or received.",
            SenderKind::AccountingOrAdviser => "An accountant, bookkeeper, tax preparer, financial planner or bank adviser writing about the recipient's finances.",
            SenderKind::Retailer => "A store or consumer brand: orders, receipts, promotions, loyalty.",
            SenderKind::Newsletter => "A publication, newsletter, community digest or mailing list.",
            SenderKind::Government => "A government body, agency or public service.",
            SenderKind::PersonOrOther => "A person writing as themselves, or an organisation none of the other choices fits.",
        }
    }

    /// For a sentence: "a cloud or hosting provider".
    pub fn words(self) -> &'static str {
        match self {
            SenderKind::CloudOrHosting => "a cloud or hosting provider",
            SenderKind::DomainRegistrar => "a domain registrar",
            SenderKind::DeveloperTooling => "a developer tooling vendor",
            SenderKind::PaymentsService => "a payments service",
            SenderKind::AccountingOrAdviser => "an accountant or financial adviser",
            SenderKind::Retailer => "a retailer",
            SenderKind::Newsletter => "a newsletter",
            SenderKind::Government => "a government body",
            SenderKind::PersonOrOther => "a person or something else",
        }
    }
}

/// A sender's judged kind, kept per domain in the index store so each
/// domain costs one judge call however much mail it sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenderKindVerdict {
    pub domain: String,
    pub kind: SenderKind,
    /// The judge's probability for the kind it chose.
    pub score: f64,
    /// The message it was judged from.
    pub message_id: MessageId,
    pub judged_at: DateTime<Utc>,
}

impl SenderKindVerdict {
    pub fn key(domain: &str) -> String {
        format!("sender_kind:v1:{}", domain.trim().to_ascii_lowercase())
    }
}

/// A sender Riddle noticed and the list it looks like it belongs on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SenderSuggestion {
    pub list: String,
    /// What would join the list: the sender's domain.
    pub entry: String,
    pub sender: String,
    /// Messages in the window.
    pub messages: u32,
    #[ts(optional)]
    pub kind: Option<SenderKind>,
    /// What the offer rests on, for the Director.
    pub evidence: String,
    /// The sentence the Director says.
    pub text: String,
}

/// Everything Riddle noticed about its rules, for the Director page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Suggestions {
    pub series: Vec<SeriesSuggestion>,
    pub senders: Vec<SenderSuggestion>,
}

/// A sender whose open Finds repeat one subject, numbers aside, under a
/// route without `+ Replaces`. A smart lock's "batteries are critically
/// low" every few days stood as 33 Do Finds, and a registrar's reminders
/// as 30, before anyone noticed (16 Oct 2026); the fix each time was
/// `+ Replaces: same subject` on the route, which the Director can now
/// propose for any sender from this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SeriesSuggestion {
    pub mesh_id: MeshId,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    /// The route as written, when the Mesh still has it.
    pub route: Option<String>,
    pub sender: String,
    /// Open Finds in the runs.
    pub count: u32,
    /// Runs side by side, one per subject with its numbers set aside.
    pub runs: u32,
    /// One subject from the largest run.
    pub sample: String,
    /// The sentence the Director says.
    pub text: String,
    pub find_ids: Vec<FindId>,
}
