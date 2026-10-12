//! Places: where the Inbox's mail column can take you. The Inbox filtered
//! by label from a flat dropdown of 32 Gmail labels, with Riddle's Strata in
//! a second dropdown beside it; a person who kept a `Receipts` label met
//! Riddle's `Receipts` Stratum as a different thing with a different count
//! (mail-navigation study, option D, 11 Oct 2026). A place is one of four
//! kinds, named so the same place means the same mail in every account:
//!
//! - a mailbox, Gmail's own `INBOX`, `STARRED` and `SENT`;
//! - a label, one of the person's own Gmail labels by its full name, so
//!   `Work/Receipts` in two accounts is one place;
//! - a Topic, a Stratum together with the person's label of the same name
//!   (compared without case), so the label and the Stratum are one row;
//! - all mail, which filters nothing.
//!
//! On the wire a place is a string (`label:Work/Receipts`, `topic:Receipts`,
//! `mailbox:INBOX`, `all`), which is what the URL carries and what a pin
//! stores.

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(try_from = "String", into = "String")]
#[ts(export, type = "string")]
pub enum Place {
    All,
    Mailbox(String),
    Label(String),
    Topic(String),
}

impl Place {
    pub fn kind(&self) -> PlaceKind {
        match self {
            Place::All | Place::Mailbox(_) => PlaceKind::Mailbox,
            Place::Label(_) => PlaceKind::Label,
            Place::Topic(_) => PlaceKind::Topic,
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::All => f.write_str("all"),
            Place::Mailbox(id) => write!(f, "mailbox:{id}"),
            Place::Label(name) => write!(f, "label:{name}"),
            Place::Topic(name) => write!(f, "topic:{name}"),
        }
    }
}

impl std::str::FromStr for Place {
    type Err = riddle_errors::RiddleError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || {
            riddle_errors::RiddleError::invalid(format!(
                "not a place: {s:?} (all, mailbox:…, label:… or topic:…)"
            ))
        };
        if s == "all" {
            return Ok(Place::All);
        }
        let (kind, name) = s.split_once(':').ok_or_else(bad)?;
        if name.trim().is_empty() {
            return Err(bad());
        }
        match kind {
            "mailbox" => Ok(Place::Mailbox(name.to_string())),
            "label" => Ok(Place::Label(name.to_string())),
            "topic" => Ok(Place::Topic(name.to_string())),
            _ => Err(bad()),
        }
    }
}

impl TryFrom<String> for Place {
    type Error = riddle_errors::RiddleError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Place> for String {
    fn from(p: Place) -> String {
        p.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PlaceKind {
    Mailbox,
    Label,
    Topic,
}

/// One row of the mail column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailPlace {
    pub place: Place,
    pub kind: PlaceKind,
    /// The full name: `Inbox`, `Work/Northwind/Github`, `Receipts`.
    pub name: String,
    /// A Topic that is also one of the person's labels.
    pub has_label: bool,
    #[ts(type = "number")]
    pub messages: u64,
    #[ts(type = "number")]
    pub unread: u64,
    pub pinned: bool,
}

/// The mail column: mailboxes first, then the person's pins in the order
/// they pinned them, then Topics by size, then the remaining labels in tree
/// order (empty ones included; the column folds them). A pinned place is
/// listed under Pinned only.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailPlaces {
    pub mailboxes: Vec<MailPlace>,
    pub pinned: Vec<MailPlace>,
    pub topics: Vec<MailPlace>,
    pub labels: Vec<MailPlace>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_reads_and_writes_as_its_string() {
        for s in [
            "all",
            "mailbox:INBOX",
            "label:Work/Receipts",
            "topic:Receipts",
            "label:a:b",
        ] {
            let p: Place = s.parse().unwrap();
            assert_eq!(p.to_string(), s);
            assert_eq!(serde_json::to_value(&p).unwrap(), serde_json::json!(s));
        }
        assert_eq!(
            "label:a:b".parse::<Place>().unwrap(),
            Place::Label("a:b".into())
        );
    }

    #[test]
    fn a_string_that_is_not_a_place_is_refused() {
        for s in ["", "inbox", "label:", "folder:x", "topic: "] {
            assert!(s.parse::<Place>().is_err(), "{s}");
        }
    }
}
