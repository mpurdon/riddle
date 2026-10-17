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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
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

/// Each kind once: its name as the judge and serde know it, the choice as
/// the judge is given it, and the words for a sentence.
const KINDS: [(SenderKind, &str, &str, &str); 9] = [
    (SenderKind::CloudOrHosting, "cloud_or_hosting", "A cloud, hosting or infrastructure provider: compute, storage, DNS hosting, CDN, servers, usage statements.", "a cloud or hosting provider"),
    (SenderKind::DomainRegistrar, "domain_registrar", "A domain name registrar: domain registrations, renewals, expiries, transfers.", "a domain registrar"),
    (SenderKind::DeveloperTooling, "developer_tooling", "A developer tool or service: code hosting, CI, monitoring, design or modelling tools, APIs, developer subscriptions.", "a developer tooling vendor"),
    (SenderKind::PaymentsService, "payments_service", "A service that moves money for the recipient: e-transfers, PayPal, Wise, a processor's notice that money was sent or received.", "a payments service"),
    (SenderKind::AccountingOrAdviser, "accounting_or_adviser", "An accountant, bookkeeper, tax preparer, financial planner or bank adviser writing about the recipient's finances.", "an accountant or financial adviser"),
    (SenderKind::Retailer, "retailer", "A store or consumer brand: orders, receipts, promotions, loyalty.", "a retailer"),
    (SenderKind::Newsletter, "newsletter", "A publication, newsletter, community digest or mailing list.", "a newsletter"),
    (SenderKind::Government, "government", "A government body, agency or public service.", "a government body"),
    (SenderKind::PersonOrOther, "person_or_other", "A person writing as themselves, or an organisation none of the other choices fits.", "a person or something else"),
];

impl SenderKind {
    /// Every kind, in the order the judge is offered them.
    pub const ALL: [SenderKind; 9] = [
        KINDS[0].0, KINDS[1].0, KINDS[2].0, KINDS[3].0, KINDS[4].0, KINDS[5].0, KINDS[6].0,
        KINDS[7].0, KINDS[8].0,
    ];

    fn spec(self) -> &'static (SenderKind, &'static str, &'static str, &'static str) {
        &KINDS[self as usize]
    }

    pub fn as_str(self) -> &'static str {
        self.spec().1
    }

    pub fn parse(s: &str) -> Option<Self> {
        KINDS.iter().find(|k| k.1 == s).map(|k| k.0)
    }

    /// The choice as the judge is given it.
    pub fn definition(self) -> &'static str {
        self.spec().2
    }

    /// For a sentence: "a cloud or hosting provider".
    pub fn words(self) -> &'static str {
        self.spec().3
    }
}

/// A sender's judged kind, kept per domain in the index store so each
/// domain costs one judgment however much mail it sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenderKindVerdict {
    pub domain: String,
    /// The kind the first message judged was given, whether or not the
    /// second agreed.
    pub kind: SenderKind,
    /// The judge's probability for its kind; the lower of two when two
    /// messages were judged.
    pub score: f64,
    /// Whether every message judged said the same kind. A sender that
    /// writes often is judged on two of its messages, since one message
    /// from a mixed domain (google.com) says little about the rest; a
    /// disagreement is kept, so the domain is not judged again, and offers
    /// nothing.
    pub agreed: bool,
    /// The judge that said so (`JudgeIdentity::key`); a verdict from
    /// another judge does not count, so a model change judges afresh.
    pub engine: String,
    /// The message it was judged from (the newest, when two were).
    pub message_id: MessageId,
    pub judged_at: DateTime<Utc>,
}

impl SenderKindVerdict {
    /// v3: the judge's identity on the verdict.
    pub const PREFIX: &'static str = "sender_kind:v4:";

    pub fn key(domain: &str) -> String {
        format!("{}{}", Self::PREFIX, domain.trim().to_ascii_lowercase())
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
    /// What it looks like, for a sentence: "looks like a domain registrar".
    pub why: String,
    /// What the offer rests on, for the Director.
    pub evidence: String,
    /// The sentence the Director says.
    pub text: String,
    /// The line on the Director page.
    pub label: String,
    /// The question the page puts in the composer.
    pub prompt: String,
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
    /// The line on the Director page.
    pub label: String,
    /// The question the page puts in the composer.
    pub prompt: String,
    pub find_ids: Vec<FindId>,
}
