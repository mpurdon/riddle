//! Named lists: the user's own sets of senders and domains that Meshes name
//! as `sender IN list:banks`. They are what keeps a rule pack general: the
//! Shopping pack says "keep promotions from `list:keep_promotions`" and the
//! person fills the list in, instead of the rule text carrying one person's
//! `uber.com`, `linkedin.com` and `olg.ca` as it did before packs.
//!
//! An entry with a local part (`billing@moniker.com`) matches that address.
//! Any other entry (`uber.com`, `@uber.com`) is a domain and matches it and
//! its subdomains (`email.uber.com`), by whole labels, so `uber.com` does
//! not match `notuber.com`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// One list as stored and shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NamedListDef {
    /// `banks`: lowercase letters, digits and `_`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Addresses and domains, lowercase, in the order the user gave them.
    pub entries: Vec<String>,
    /// The pack that asked for it, if any.
    #[serde(default)]
    pub pack: Option<String>,
}

impl NamedListDef {
    /// Lowercase, trimmed, without a leading `@`, empty entries dropped,
    /// duplicates removed in order.
    pub fn normalise(entries: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in entries {
            let e = e.trim().trim_start_matches('@').to_ascii_lowercase();
            if !e.is_empty() && !out.contains(&e) {
                out.push(e);
            }
        }
        out
    }

    /// Whether a list name is well formed.
    pub fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 40
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }
}

/// Every list by name, for evaluation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NamedLists(pub BTreeMap<String, Vec<String>>);

impl NamedLists {
    pub fn from_defs(defs: impl IntoIterator<Item = NamedListDef>) -> Self {
        Self(defs.into_iter().map(|d| (d.name, d.entries)).collect())
    }

    pub fn exists(&self, list: &str) -> bool {
        self.0.contains_key(list)
    }

    /// Whether the sender's address is on the list (see the module docs).
    pub fn contains(&self, list: &str, sender: &str) -> bool {
        let Some(entries) = self.0.get(list) else {
            return false;
        };
        let sender = sender.trim().to_ascii_lowercase();
        let domain = sender.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
        entries.iter().any(|e| {
            if e.contains('@') {
                *e == sender
            } else {
                domain == e
                    || domain
                        .strip_suffix(e.as_str())
                        .is_some_and(|rest| rest.ends_with('.'))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_matches_itself_and_a_domain_matches_its_subdomains_by_whole_labels() {
        let lists = NamedLists::from_defs([NamedListDef {
            name: "keep".into(),
            description: String::new(),
            entries: NamedListDef::normalise(&[
                "@Uber.com".into(),
                "billing@moniker.com".into(),
                " ".into(),
            ]),
            pack: None,
        }]);
        assert!(lists.contains("keep", "noreply@uber.com"));
        assert!(lists.contains("keep", "a@email.uber.com"));
        assert!(!lists.contains("keep", "a@notuber.com"));
        assert!(lists.contains("keep", "Billing@Moniker.com"));
        assert!(!lists.contains("keep", "support@moniker.com"));
        assert!(!lists.contains("missing", "noreply@uber.com"));
    }
}
