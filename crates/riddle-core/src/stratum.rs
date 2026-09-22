//! Strata: overlapping semantic categories, governed and versioned (§2).

use crate::ids::StratumId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Stratum {
    pub id: StratumId,
    pub name: String,
    pub version: u32,
    /// `None` is global; otherwise a Dig Group name.
    pub scope_group: Option<String>,
    pub description: String,
    pub proposed_by_riddle: bool,
    pub message_count: u64,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Stratum {
    pub const DEFAULTS: &'static [(&'static str, &'static str)] = &[
        ("People", "Correspondence with individuals"),
        ("Projects", "Mail about active projects"),
        ("Purchases", "Orders, shipping and receipts"),
        ("Travel", "Reservations, itineraries, boarding passes"),
        ("Finance", "Banking, invoices, statements, payments"),
        ("Engineering", "Code review, CI, deployments, incidents"),
        ("Notifications", "Automated service notifications"),
        ("Newsletters", "Periodic editorial mail"),
        ("Receipts", "Proof of purchase"),
        ("Needs Response", "Someone is waiting on the user"),
    ];
}
