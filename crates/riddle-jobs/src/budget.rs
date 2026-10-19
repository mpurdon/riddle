//! The daily spend budgets jobs yield to (§55, §56).
//!
//! `BudgetGate::remaining` is asked before a budgeted job starts. When it
//! is exhausted, BACKFILL and EXPERIMENT jobs are held, which the runner
//! shows as "budget"; higher classes proceed, because a user-triggered
//! Sift still needs an answer and the judge crate applies its own hard cap
//! on the usage ledger. Pacing against the Gmail quota and the judge's and
//! LLM's rate limits lives in each client; a priority-aware gate per
//! resource once lived here too, never acquired, and went (19 Oct 2026).

use parking_lot::RwLock;
use riddle_core::{DigId, Priority};
use riddle_store::{repo, Stores};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BudgetClass {
    Judge,
    Llm,
}

/// Remaining USD today for a Dig (or across Digs when `None`). The
/// integrator builds it from settings and the usage ledger; see
/// `UsageBudgetGate`.
pub trait BudgetGate: Send + Sync {
    fn remaining(&self, dig: Option<&DigId>, class: BudgetClass) -> f64;
}

/// Classes that pause first when a budget is exhausted (§55, §94).
pub fn yields_to_budget(priority: Priority) -> bool {
    matches!(priority, Priority::Backfill | Priority::Experiment)
}

/// No budget: everything proceeds.
pub struct UnlimitedBudget;
impl BudgetGate for UnlimitedBudget {
    fn remaining(&self, _dig: Option<&DigId>, _class: BudgetClass) -> f64 {
        f64::INFINITY
    }
}

/// Daily USD limits against the usage ledger of each open Dig store.
pub struct UsageBudgetGate {
    stores: Arc<Stores>,
    limits: RwLock<(f64, f64)>,
}

impl UsageBudgetGate {
    pub fn new(stores: Arc<Stores>, daily_judge_usd: f64, daily_llm_usd: f64) -> Self {
        Self {
            stores,
            limits: RwLock::new((daily_judge_usd, daily_llm_usd)),
        }
    }
    /// Called when settings change.
    pub fn set_limits(&self, daily_judge_usd: f64, daily_llm_usd: f64) {
        *self.limits.write() = (daily_judge_usd, daily_llm_usd);
    }
    fn spent(&self, store: &riddle_store::Store, class: BudgetClass) -> f64 {
        let providers: &[&str] = match class {
            BudgetClass::Judge => &["jev"],
            BudgetClass::Llm => &["bedrock"],
        };
        repo::usage::spent_today_by_provider(store, providers).unwrap_or(0.0)
    }
}

impl BudgetGate for UsageBudgetGate {
    fn remaining(&self, dig: Option<&DigId>, class: BudgetClass) -> f64 {
        let (judge, llm) = *self.limits.read();
        let limit = match class {
            BudgetClass::Judge => judge,
            BudgetClass::Llm => llm,
        };
        let spent = match dig {
            Some(d) => self
                .stores
                .dig(d)
                .map(|s| self.spent(&s, class))
                .unwrap_or(0.0),
            None => self
                .stores
                .open_digs()
                .iter()
                .map(|(_, s)| self.spent(s, class))
                .sum(),
        };
        limit - spent
    }
}
