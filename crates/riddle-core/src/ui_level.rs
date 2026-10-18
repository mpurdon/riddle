//! How much of Riddle a person sees: `UiLevel`, a setting the backend owns.
//!
//! The rail showed sixteen entries to someone who wanted to read their mail
//! (17 Oct 2026): Meshes, Packs, Rule check, Strata, Signals and Expeditions
//! sat beside Finds and the Inbox, and the Director answered a question about
//! a newsletter with Signal versions and route syntax. The level is one
//! switch with three rungs. Essentials is the brief, the mail, what was set
//! aside and settings; Everything adds rule authoring, the taxonomy and long
//! jobs; Developer adds calibration, traces and the arming switch. Every
//! route stays registered at every level: a link still opens an advanced
//! page, and nothing that keeps mail recoverable (Never Hidden, Unresolved,
//! Spoil and Contamination review, Undo) is ever gated.
//!
//! It lives here rather than in riddle-settings because the Director changes
//! register with it and the CLI and MCP read the same value, and none of
//! those depend on the settings crate. `Developer` is the one rung that
//! implies the older `developer_mode` flag; the settings crate derives the
//! flag from the level so the backend's existing readers keep working.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS, Default,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UiLevel {
    /// The brief, the mail, what was set aside, and settings.
    #[default]
    Essentials,
    /// Rules, packs, topics, labels and long jobs as well.
    Everything,
    /// Signals, traces, inspectors and the dev build's arming switch too.
    Developer,
}

impl UiLevel {
    pub const ALL: [UiLevel; 3] = [UiLevel::Essentials, UiLevel::Everything, UiLevel::Developer];

    pub fn as_str(self) -> &'static str {
        match self {
            UiLevel::Essentials => "essentials",
            UiLevel::Everything => "everything",
            UiLevel::Developer => "developer",
        }
    }

    /// True when this level shows what `needed` gates: Developer implies
    /// Everything, which implies Essentials.
    pub fn shows(self, needed: UiLevel) -> bool {
        self >= needed
    }

    pub fn is_developer(self) -> bool {
        self == UiLevel::Developer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_implies_everything_which_implies_essentials() {
        assert!(UiLevel::Developer.shows(UiLevel::Everything));
        assert!(UiLevel::Developer.shows(UiLevel::Essentials));
        assert!(UiLevel::Everything.shows(UiLevel::Essentials));
        assert!(!UiLevel::Everything.shows(UiLevel::Developer));
        assert!(!UiLevel::Essentials.shows(UiLevel::Everything));
        assert!(UiLevel::Developer.is_developer());
        assert!(!UiLevel::Everything.is_developer());
    }

    #[test]
    fn the_level_serialises_as_a_lowercase_word_and_defaults_to_essentials() {
        assert_eq!(
            serde_json::to_string(&UiLevel::Everything).unwrap(),
            "\"everything\""
        );
        let back: UiLevel = serde_json::from_str("\"developer\"").unwrap();
        assert_eq!(back, UiLevel::Developer);
        assert_eq!(UiLevel::default(), UiLevel::Essentials);
        assert_eq!(UiLevel::Essentials.as_str(), "essentials");
    }
}
