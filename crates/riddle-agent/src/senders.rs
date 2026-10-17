//! Sender discovery: a sender with enough mail that is on no list is offered
//! the list it looks like it belongs on, from what the store already says
//! (invoices scored, promotions the user restored) and from one judgment per
//! domain of what kind of organisation sends the mail. Lists were filled by
//! hand at install and by hand again when a registrar, a cloud provider and
//! a tooling vendor turned up as plain bills (17 Oct 2026). Pure: the app
//! assembles the rows and asks the judge; this maps evidence to a proposal
//! the Director voices, and nothing joins a list without the user's yes.

use riddle_core::{JudgeQuestion, SenderKind, SenderSuggestion, SignalPrimitive};
use std::collections::{BTreeMap, BTreeSet};

/// Messages from a sender in the window before it is worth a word.
pub const SUGGEST_AT: u32 = 3;
/// The judge's confidence in a sender's kind before it is acted on.
pub const KIND_AT: f64 = 0.6;
/// Invoices scored from a sender before it is an invoice sender.
pub const INVOICES_AT: u32 = 3;
/// Promotions restored from Spoil before the sender is one to keep.
pub const RESTORED_AT: u32 = 2;

/// The judge's question, one per sender domain on one of its messages.
pub const SENDER_KIND_QUESTION: &str = "sender_kind@v1";

pub fn sender_kind_question() -> JudgeQuestion {
    JudgeQuestion {
        id: SENDER_KIND_QUESTION.into(),
        primitive: SignalPrimitive::Choice,
        definition: "What kind of organisation sent this message, judged from the message alone: the service it sells or provides, not the subject of this one message.".into(),
        options: SenderKind::ALL
            .iter()
            .map(|k| (k.as_str().to_string(), k.definition().to_string()))
            .collect(),
    }
}

/// One sender as the detector sees it; the app assembles these.
#[derive(Debug, Clone)]
pub struct SenderRow {
    pub sender: String,
    pub domain: String,
    /// Messages in the window.
    pub messages: u32,
    /// The judged kind and the judge's confidence, when judged.
    pub kind: Option<(SenderKind, f64)>,
    /// Recent messages scored as an invoice (an amount due).
    pub invoices: u32,
    /// Promotions of theirs the user restored from Spoil.
    pub restored_promotions: u32,
    /// Lists the sender is already on.
    pub lists: Vec<String>,
}

/// Which list a kind of sender belongs on, when the list exists.
fn list_for_kind(kind: SenderKind) -> Option<&'static str> {
    match kind {
        SenderKind::CloudOrHosting | SenderKind::DomainRegistrar | SenderKind::DeveloperTooling => {
            Some("engineering_vendors")
        }
        SenderKind::PaymentsService => Some("money_transfers"),
        SenderKind::AccountingOrAdviser => Some("financial_advisors"),
        SenderKind::Retailer
        | SenderKind::Newsletter
        | SenderKind::Government
        | SenderKind::PersonOrOther => None,
    }
}

/// The part of a domain a list entry should name: `fb02.freshbooks.com` and
/// `fb05.freshbooks.com` are one sender, `freshbooks.com`, and a list entry
/// matches its subdomains. The last two labels, or three under a country
/// code's second level (`example.co.uk`); a public-suffix list would be
/// exact and is not worth carrying for this.
pub fn registrable_domain(domain: &str) -> String {
    let domain = domain.trim().trim_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = domain.split('.').collect();
    let n = labels.len();
    if n <= 2 {
        return domain;
    }
    let country_second_level = labels[n - 1].len() == 2
        && matches!(
            labels[n - 2],
            "co" | "com" | "net" | "org" | "gov" | "edu" | "ac" | "gc"
        );
    let keep = if country_second_level { 3 } else { 2 };
    labels[n - keep..].join(".")
}

/// Map each sender's evidence to the lists it looks like it belongs on,
/// among the lists that exist, skipping lists it is already on. Sibling
/// subdomains whose judged kinds agree are one offer for the registrable
/// domain; when they disagree (a retailer's app store beside its shop),
/// each is offered on its own.
pub fn suggest_lists<'a>(
    rows: &'a [SenderRow],
    lists_present: &'a [String],
) -> Vec<SenderSuggestion> {
    let present: BTreeSet<&'a str> = lists_present.iter().map(String::as_str).collect();
    // Judged kinds under each registrable domain: do its subdomains speak as one?
    let mut kinds_by_root: BTreeMap<String, BTreeSet<SenderKind>> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.messages >= SUGGEST_AT) {
        if let Some((kind, score)) = r.kind {
            if score >= KIND_AT {
                kinds_by_root
                    .entry(registrable_domain(&r.domain))
                    .or_default()
                    .insert(kind);
            }
        }
    }
    struct Offer<'a> {
        list: &'static str,
        entry: String,
        best: &'a SenderRow,
        messages: u32,
        kind: Option<SenderKind>,
        evidence: String,
        why: String,
    }
    /// The offers so far, merging sibling subdomains that speak as one.
    struct Offers<'a> {
        present: BTreeSet<&'a str>,
        kinds_by_root: BTreeMap<String, BTreeSet<SenderKind>>,
        map: BTreeMap<(&'static str, String), Offer<'a>>,
    }
    impl<'a> Offers<'a> {
        fn push(
            &mut self,
            list: &'static str,
            row: &'a SenderRow,
            kind: Option<SenderKind>,
            evidence: String,
            why: String,
        ) {
            if !self.present.contains(list) || row.lists.iter().any(|l| l == list) {
                return;
            }
            let root = registrable_domain(&row.domain);
            let one_voice = self.kinds_by_root.get(&root).is_none_or(|k| k.len() <= 1);
            let entry = if one_voice { root } else { row.domain.clone() };
            match self.map.entry((list, entry.clone())) {
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    let o = e.get_mut();
                    o.messages += row.messages;
                    if row.messages > o.best.messages {
                        o.best = row;
                        o.kind = kind;
                        o.evidence = evidence;
                        o.why = why;
                    }
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(Offer {
                        list,
                        entry,
                        best: row,
                        messages: row.messages,
                        kind,
                        evidence,
                        why,
                    });
                }
            }
        }
    }
    let mut offers = Offers {
        present,
        kinds_by_root,
        map: BTreeMap::new(),
    };
    for row in rows.iter().filter(|r| r.messages >= SUGGEST_AT) {
        if let Some((kind, score)) = row.kind {
            if score >= KIND_AT {
                if let Some(list) = list_for_kind(kind) {
                    offers.push(
                        list,
                        row,
                        Some(kind),
                        format!("judged {} ({score:.2}) from one message", kind.as_str()),
                        format!("looks like {}", kind.words()),
                    );
                }
            }
        }
        if row.invoices >= INVOICES_AT {
            offers.push(
                "invoice_senders",
                row,
                row.kind.map(|(k, _)| k),
                format!("{} recent messages scored as an invoice", row.invoices),
                format!(
                    "sends invoices ({} of its recent messages present an amount due)",
                    row.invoices
                ),
            );
        }
        if row.restored_promotions >= RESTORED_AT {
            offers.push(
                "keep_promotions",
                row,
                row.kind.map(|(k, _)| k),
                format!("{} promotions restored from Spoil", row.restored_promotions),
                format!(
                    "sends promotions you have restored from Spoil {} times",
                    row.restored_promotions
                ),
            );
        }
    }
    let mut out: Vec<SenderSuggestion> = offers
        .map
        .into_values()
        .map(|o| SenderSuggestion {
            text: format!(
                "{} {} and is on no list ({} in the last four months). Add it to {}? I can propose the list change and show what re-deciding its mail would do.",
                o.entry,
                o.why,
                riddle_core::plural(o.messages as usize, "message"),
                o.list
            ),
            list: o.list.to_string(),
            entry: o.entry,
            sender: o.best.sender.clone(),
            messages: o.messages,
            kind: o.kind,
            evidence: o.evidence,
        })
        .collect();
    out.sort_by(|a, b| b.messages.cmp(&a.messages).then(a.entry.cmp(&b.entry)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(domain: &str, messages: u32, kind: Option<(SenderKind, f64)>) -> SenderRow {
        SenderRow {
            sender: format!("billing@{domain}"),
            domain: domain.into(),
            messages,
            kind,
            invoices: 0,
            restored_promotions: 0,
            lists: vec![],
        }
    }

    fn lists() -> Vec<String> {
        [
            "engineering_vendors",
            "invoice_senders",
            "keep_promotions",
            "money_transfers",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn a_cloud_provider_with_enough_mail_is_offered_the_engineering_vendors_list() {
        let rows = [
            row(
                "amazonaws.com",
                11,
                Some((SenderKind::CloudOrHosting, 0.91)),
            ),
            row("gmail.com", 40, Some((SenderKind::PersonOrOther, 0.95))),
            row(
                "newvendor.example",
                2,
                Some((SenderKind::DeveloperTooling, 0.9)),
            ),
            row(
                "unsure.example",
                9,
                Some((SenderKind::DomainRegistrar, 0.4)),
            ),
        ];
        let s = suggest_lists(&rows, &lists());
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!(
            (s[0].list.as_str(), s[0].entry.as_str()),
            ("engineering_vendors", "amazonaws.com")
        );
        assert_eq!(s[0].kind, Some(SenderKind::CloudOrHosting));
        assert!(
            s[0].text.contains("cloud or hosting provider"),
            "{}",
            s[0].text
        );
        assert!(s[0].text.contains("11 messages"), "{}", s[0].text);
    }

    #[test]
    fn a_sender_already_on_the_list_or_a_list_that_does_not_exist_is_not_offered() {
        let mut on = row("moniker.com", 30, Some((SenderKind::DomainRegistrar, 0.9)));
        on.lists = vec!["engineering_vendors".into()];
        assert!(suggest_lists(&[on.clone()], &lists()).is_empty());
        on.lists.clear();
        assert!(suggest_lists(&[on], &["keep_promotions".to_string()]).is_empty());
    }

    #[test]
    fn invoices_and_restored_promotions_count_without_a_judgment() {
        let mut inv = row("ledgerly.example", 5, None);
        inv.invoices = 3;
        let mut promo = row("shop.example", 12, None);
        promo.restored_promotions = 2;
        let s = suggest_lists(&[inv, promo], &lists());
        let lists_offered: Vec<&str> = s.iter().map(|x| x.list.as_str()).collect();
        assert_eq!(
            lists_offered,
            ["keep_promotions", "invoice_senders"],
            "{s:?}"
        );
        assert!(
            s[1].text.contains("3 of its recent messages"),
            "{}",
            s[1].text
        );
    }

    #[test]
    fn a_list_entry_names_the_registrable_domain() {
        assert_eq!(registrable_domain("fb02.freshbooks.com"), "freshbooks.com");
        assert_eq!(registrable_domain("github.com"), "github.com");
        assert_eq!(registrable_domain("mail.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("news.kilocode.ai"), "kilocode.ai");
    }

    #[test]
    fn sibling_subdomains_that_agree_are_one_offer_and_ones_that_disagree_stand_apart() {
        let rows = [
            row(
                "email.github.com",
                6,
                Some((SenderKind::DeveloperTooling, 0.9)),
            ),
            row("github.com", 5, Some((SenderKind::DeveloperTooling, 1.0))),
            row(
                "appstore.amazon.com",
                3,
                Some((SenderKind::DeveloperTooling, 0.85)),
            ),
            row("amazon.com", 20, Some((SenderKind::Retailer, 0.9))),
        ];
        let s = suggest_lists(&rows, &lists());
        let entries: Vec<(&str, u32)> = s.iter().map(|x| (x.entry.as_str(), x.messages)).collect();
        assert_eq!(
            entries,
            [("github.com", 11), ("appstore.amazon.com", 3)],
            "{s:?}"
        );
        assert_eq!(s[0].sender, "billing@email.github.com");
        assert!(
            s[0].text
                .starts_with("github.com looks like a developer tooling vendor"),
            "{}",
            s[0].text
        );
    }

    #[test]
    fn the_judge_question_offers_every_kind_as_a_choice() {
        let q = sender_kind_question();
        assert_eq!(q.primitive, SignalPrimitive::Choice);
        assert_eq!(q.options.len(), SenderKind::ALL.len());
        assert_eq!(q.options[0].0, "cloud_or_hosting");
    }
}
