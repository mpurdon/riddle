//! Sender discovery: a sender with enough mail that is on no list is offered
//! the list it looks like it belongs on, from what the store already says
//! (invoices scored, promotions the user restored) and from a judgment per
//! domain of what kind of organisation sends the mail. Lists were filled by
//! hand at install and by hand again when a registrar, a cloud provider and
//! a tooling vendor turned up as plain bills (17 Oct 2026). Pure: the app
//! assembles the rows and asks the judge; this maps evidence to a proposal
//! the Director voices, and nothing joins a list without the user's yes.
//! Which kinds belong on a list is the list's own declaration (`kinds`, from
//! its pack), not a table here.

use riddle_core::{
    plural, registrable_domain, JudgeQuestion, NamedListDef, SenderKind, SenderSuggestion,
    SignalPrimitive,
};
use std::collections::{BTreeMap, BTreeSet};

/// Messages from a sender in the window before it is worth a word.
pub const SUGGEST_AT: u32 = 3;
/// The judge's confidence in a sender's kind before it is acted on.
pub const KIND_AT: f64 = 0.6;
/// A sender's newest messages that count towards being an invoice sender,
/// and how many of them must be invoices: half. A sender with years of
/// promotions and a bill now and then is not an invoice sender.
pub const RECENT_PER_SENDER: usize = 6;
pub const INVOICES_AT: u32 = 3;
/// Promotions restored from Spoil before the sender is one to keep.
pub const RESTORED_AT: u32 = 2;

/// The judge's question, one per sender domain on one of its messages.
pub const SENDER_KIND_QUESTION: &str = "sender_kind@v1";

pub fn sender_kind_question() -> JudgeQuestion {
    JudgeQuestion {
        id: SENDER_KIND_QUESTION.into(),
        primitive: SignalPrimitive::Choice,
        definition: "What kind of organisation sent this message, judged from the message alone: the service it sells or provides, rather than the subject of this one message.".into(),
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
    /// The judged kind and the judge's confidence, when the judge was sure
    /// of it (two messages agreed, when two were asked).
    pub kind: Option<(SenderKind, f64)>,
    /// Recent messages scored as an invoice (an amount due).
    pub invoices: u32,
    /// Promotions of theirs the user restored from Spoil.
    pub restored_promotions: u32,
    /// Lists the sender is already on.
    pub lists: Vec<String>,
}

impl SenderRow {
    /// The judged kind, when the judge was confident enough to act on.
    pub fn judged(&self) -> Option<SenderKind> {
        self.kind
            .filter(|(_, score)| *score >= KIND_AT)
            .map(|(k, _)| k)
    }
}

/// One reason to offer one sender one list.
struct Candidate<'a> {
    list: String,
    entry: String,
    row: &'a SenderRow,
    why: String,
    evidence: String,
}

/// Map each sender's evidence to the lists it looks like it belongs on,
/// among `lists`, skipping lists it is already on. Sibling subdomains whose
/// judged kinds agree are one offer for the registrable domain; when they
/// disagree (a retailer's app store beside its shop), each is offered on
/// its own.
pub fn suggest_lists(rows: &[SenderRow], lists: &[NamedListDef]) -> Vec<SenderSuggestion> {
    let active: Vec<&SenderRow> = rows.iter().filter(|r| r.messages >= SUGGEST_AT).collect();
    // Judged kinds under each registrable domain: do its subdomains speak as one?
    let mut kinds_by_root: BTreeMap<String, BTreeSet<SenderKind>> = BTreeMap::new();
    for r in &active {
        if let Some(kind) = r.judged() {
            kinds_by_root
                .entry(registrable_domain(&r.domain))
                .or_default()
                .insert(kind);
        }
    }
    let entry_for = |r: &SenderRow| {
        let root = registrable_domain(&r.domain);
        if kinds_by_root.get(&root).is_none_or(|k| k.len() <= 1) {
            root
        } else {
            r.domain.clone()
        }
    };
    let exists = |name: &str| lists.iter().any(|l| l.name == name);

    let mut candidates: Vec<Candidate<'_>> = Vec::new();
    for r in &active {
        let mut offer = |list: &str, why: String, evidence: String| {
            if exists(list) && !r.lists.iter().any(|l| l == list) {
                candidates.push(Candidate {
                    list: list.to_string(),
                    entry: entry_for(r),
                    row: r,
                    why,
                    evidence,
                });
            }
        };
        if let (Some(kind), Some((_, score))) = (r.judged(), r.kind) {
            for list in lists.iter().filter(|l| l.kinds.contains(&kind)) {
                offer(
                    &list.name,
                    format!("looks like {}", kind.words()),
                    format!("judged {} ({score:.2})", kind.as_str()),
                );
            }
        }
        if r.invoices >= INVOICES_AT {
            offer(
                "invoice_senders",
                format!(
                    "sends invoices ({} of its recent messages present an amount due)",
                    r.invoices
                ),
                format!("{} recent messages scored as an invoice", r.invoices),
            );
        }
        if r.restored_promotions >= RESTORED_AT {
            offer(
                "keep_promotions",
                format!(
                    "sends promotions you have restored from Spoil {} times",
                    r.restored_promotions
                ),
                format!("{} promotions restored from Spoil", r.restored_promotions),
            );
        }
    }

    // One offer per list and entry: messages summed over the senders behind
    // it, the words from the one with the most.
    let mut grouped: BTreeMap<(String, String), Vec<Candidate<'_>>> = BTreeMap::new();
    for c in candidates {
        grouped
            .entry((c.list.clone(), c.entry.clone()))
            .or_default()
            .push(c);
    }
    let mut out: Vec<SenderSuggestion> = grouped
        .into_iter()
        .map(|((list, entry), group)| {
            let messages: u32 = group.iter().map(|c| c.row.messages).sum();
            let best = group
                .iter()
                .max_by_key(|c| c.row.messages)
                .expect("a group has a member");
            let count = plural(messages as usize, "message");
            SenderSuggestion {
                label: format!("{entry} {}. Add it to {}?", best.why, list.replace('_', " ")),
                prompt: format!(
                    "{entry} {} and has sent {count} in the last four months; it is on no list. Add it to the {list} list and show me what re-deciding its mail would do before anything is applied.",
                    best.why
                ),
                text: format!(
                    "{entry} {} and is on no list ({count} in the last four months). Add it to {list}? I can propose the list change and show what re-deciding its mail would do.",
                    best.why
                ),
                sender: best.row.sender.clone(),
                kind: best.row.judged(),
                why: best.why.clone(),
                evidence: best.evidence.clone(),
                list,
                entry,
                messages,
            }
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

    fn list(name: &str, kinds: &[SenderKind]) -> NamedListDef {
        NamedListDef {
            name: name.into(),
            description: String::new(),
            entries: vec![],
            pack: None,
            kinds: kinds.to_vec(),
        }
    }

    fn lists() -> Vec<NamedListDef> {
        vec![
            list(
                "engineering_vendors",
                &[
                    SenderKind::CloudOrHosting,
                    SenderKind::DomainRegistrar,
                    SenderKind::DeveloperTooling,
                ],
            ),
            list("invoice_senders", &[]),
            list("keep_promotions", &[]),
            list("money_transfers", &[SenderKind::PaymentsService]),
        ]
    }

    #[test]
    fn a_cloud_provider_with_enough_mail_is_offered_the_list_that_declares_its_kind() {
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
        assert_eq!(s[0].why, "looks like a cloud or hosting provider");
        assert!(s[0].text.contains("11 messages"), "{}", s[0].text);
        assert_eq!(
            s[0].label,
            "amazonaws.com looks like a cloud or hosting provider. Add it to engineering vendors?"
        );
        assert!(
            s[0].prompt.contains("the engineering_vendors list"),
            "{}",
            s[0].prompt
        );
    }

    #[test]
    fn a_sender_already_on_the_list_or_a_list_that_does_not_exist_is_not_offered() {
        let mut on = row("moniker.com", 30, Some((SenderKind::DomainRegistrar, 0.9)));
        on.lists = vec!["engineering_vendors".into()];
        assert!(suggest_lists(&[on.clone()], &lists()).is_empty());
        on.lists.clear();
        assert!(suggest_lists(&[on], &[list("keep_promotions", &[])]).is_empty());
    }

    #[test]
    fn invoices_and_restored_promotions_count_without_a_judgment() {
        let mut inv = row("ledgerly.example", 5, None);
        inv.invoices = 3;
        let mut promo = row("shop.example", 12, None);
        promo.restored_promotions = 2;
        let s = suggest_lists(&[inv, promo], &lists());
        let offered: Vec<&str> = s.iter().map(|x| x.list.as_str()).collect();
        assert_eq!(offered, ["keep_promotions", "invoice_senders"], "{s:?}");
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
        assert_eq!(SenderKind::parse("retailer"), Some(SenderKind::Retailer));
    }
}
