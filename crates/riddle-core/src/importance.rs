//! How much a message matters: Low, Normal, High or Critical. Every message
//! has one. A route can set it (`+ Importance: high`); otherwise Sift
//! derives it from the decision (hidden mail is Low, a Find takes its level
//! from the priority score), and the user can set it on one message, which
//! no rule change overrides. `Priority:` in a Mesh was already the order
//! Meshes run in, so the level has its own word.
//!
//! A Find keeps its 0–100 priority for ordering within a level; the level
//! is the band the number falls in, and an explicit level moves the number
//! into its band rather than replacing it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Importance {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

impl Importance {
    pub const ALL: [Importance; 4] = [
        Importance::Low,
        Importance::Normal,
        Importance::High,
        Importance::Critical,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Importance::Low => "low",
            Importance::Normal => "normal",
            Importance::High => "high",
            Importance::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Importance::Low),
            "normal" => Some(Importance::Normal),
            "high" => Some(Importance::High),
            "critical" => Some(Importance::Critical),
            _ => None,
        }
    }

    /// The level a decision has when nothing says more: hidden mail is
    /// Low, everything else Normal.
    pub fn default_for(d: &crate::classification::Disposition) -> Self {
        use crate::classification::Disposition::*;
        match d {
            Spoil | Filed | Contamination => Importance::Low,
            Find { .. } | Unresolved => Importance::Normal,
        }
    }

    /// The level as stored in the `importance` column: 0 (Low) to 3.
    pub fn rank(self) -> u8 {
        self as u8
    }

    pub fn from_rank(n: i64) -> Self {
        match n {
            i64::MIN..=0 => Importance::Low,
            1 => Importance::Normal,
            2 => Importance::High,
            _ => Importance::Critical,
        }
    }

    /// The band a Find priority (0–100) falls in.
    pub fn from_priority(p: u8) -> Self {
        match p {
            0..=34 => Importance::Low,
            35..=69 => Importance::Normal,
            70..=89 => Importance::High,
            _ => Importance::Critical,
        }
    }

    /// `p` moved into this level's band, keeping its place where it can.
    pub fn clamp_priority(self, p: u8) -> u8 {
        let (lo, hi) = match self {
            Importance::Low => (0, 34),
            Importance::Normal => (35, 69),
            Importance::High => (70, 89),
            Importance::Critical => (90, 100),
        };
        p.clamp(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_priority_reads_as_the_band_it_falls_in_and_clamps_back_into_it() {
        assert_eq!(Importance::from_priority(50), Importance::Normal);
        assert_eq!(Importance::from_priority(95), Importance::Critical);
        for level in Importance::ALL {
            for p in [0u8, 34, 35, 69, 70, 89, 90, 100] {
                assert_eq!(Importance::from_priority(level.clamp_priority(p)), level);
            }
            assert_eq!(Importance::from_rank(level.rank() as i64), level);
            assert_eq!(Importance::parse(level.as_str()), Some(level));
        }
    }
}
