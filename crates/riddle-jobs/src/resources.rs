//! The four limited resources (§55) and the budget hook (§56).
//!
//! One `PriorityGate` per resource: a token bucket (Gmail units per Dig,
//! judge requests, LLM requests) or a single slot (the SQLite writer). The
//! gate is priority-aware: a request waits while any higher class is
//! waiting on the same gate, so a queued INTERACTIVE acquisition pre-empts
//! BACKFILL's next one even when tokens are available. This is the
//! back-pressure rule "lower classes yield" made concrete. Within one class
//! the order is whoever wakes first; Digs are not round-robined (noted as a
//! gap). Time comes from `tokio::time` so tests run under paused time.
//!
//! Budgets: `BudgetGate::remaining` is asked before a judge or LLM token is
//! taken. When it is exhausted, BACKFILL and EXPERIMENT acquisitions fail
//! with `budget_exhausted`, which the runner turns into a held job; higher
//! classes proceed, because a user-triggered Sift still needs an answer and
//! the judge crate applies its own hard cap on the usage ledger.

use parking_lot::{Mutex, RwLock};
use riddle_core::{DigId, Priority};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rdebug, Category};
use riddle_store::{repo, Stores};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    /// Gmail quota units for one Dig.
    GmailUnits,
    Judge,
    Llm,
    SqliteWriter,
}

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

const CLASSES: usize = 6;

fn class_index(p: Priority) -> usize {
    match p {
        Priority::Interactive => 0,
        Priority::Incoming => 1,
        Priority::CatchUp => 2,
        Priority::Projection => 3,
        Priority::Backfill => 4,
        Priority::Experiment => 5,
    }
}

#[derive(Debug)]
struct GateState {
    tokens: f64,
    capacity: f64,
    /// Tokens per second; zero for a slot gate.
    refill_per_sec: f64,
    last: Instant,
    waiting: [usize; CLASSES],
}

impl GateState {
    fn refill(&mut self, now: Instant) {
        if self.refill_per_sec > 0.0 {
            let dt = now.saturating_duration_since(self.last).as_secs_f64();
            self.tokens = (self.tokens + dt * self.refill_per_sec).min(self.capacity);
        }
        self.last = now;
    }
    fn higher_waiting(&self, idx: usize) -> bool {
        self.waiting[..idx].iter().any(|n| *n > 0)
    }
}

/// A token bucket or slot with class-ordered waiting.
#[derive(Debug)]
pub struct PriorityGate {
    state: Mutex<GateState>,
    notify: Notify,
    releases: bool,
}

impl PriorityGate {
    /// `per_minute` tokens, bursting to one minute's worth.
    pub fn rate(per_minute: f64) -> Self {
        let capacity = per_minute.max(1.0);
        Self {
            state: Mutex::new(GateState {
                tokens: capacity,
                capacity,
                refill_per_sec: capacity / 60.0,
                last: Instant::now(),
                waiting: [0; CLASSES],
            }),
            notify: Notify::new(),
            releases: false,
        }
    }
    /// `slots` concurrent holders; a guard returns its slot on drop.
    pub fn slots(slots: usize) -> Self {
        let capacity = slots.max(1) as f64;
        Self {
            state: Mutex::new(GateState {
                tokens: capacity,
                capacity,
                refill_per_sec: 0.0,
                last: Instant::now(),
                waiting: [0; CLASSES],
            }),
            notify: Notify::new(),
            releases: true,
        }
    }

    pub fn set_rate(&self, per_minute: f64) {
        let mut st = self.state.lock();
        let capacity = per_minute.max(1.0);
        st.capacity = capacity;
        st.refill_per_sec = capacity / 60.0;
        st.tokens = st.tokens.min(capacity);
        drop(st);
        self.notify.notify_waiters();
    }

    pub fn available(&self) -> f64 {
        let mut st = self.state.lock();
        st.refill(Instant::now());
        st.tokens
    }

    pub fn waiting(&self, priority: Priority) -> usize {
        self.state.lock().waiting[class_index(priority)]
    }

    /// Wait for `cost` tokens with class precedence. Cancel-safe: dropping
    /// the future withdraws the wait.
    pub async fn acquire(self: &Arc<Self>, priority: Priority, cost: f64) -> ResourceGuard {
        let idx = class_index(priority);
        let cost = cost.max(0.0);
        let _waiting = WaitingGuard::register(self, idx);
        loop {
            // Register before checking so a release between the check and the
            // await is not lost (`notify_waiters` stores no permit).
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let delay = {
                let mut st = self.state.lock();
                st.refill(Instant::now());
                if !st.higher_waiting(idx) && st.tokens >= cost {
                    st.tokens -= cost;
                    drop(st);
                    // Others may now be unblocked (a higher waiter left).
                    self.notify.notify_waiters();
                    return ResourceGuard {
                        gate: self.releases.then(|| self.clone()),
                        cost,
                    };
                }
                if st.refill_per_sec > 0.0 && !st.higher_waiting(idx) {
                    Duration::from_secs_f64(((cost - st.tokens) / st.refill_per_sec).max(0.001))
                } else {
                    Duration::from_secs(3600)
                }
            };
            tokio::select! {
                _ = &mut notified => {}
                _ = tokio::time::sleep(delay) => {}
            }
        }
    }

    fn release(&self, cost: f64) {
        let mut st = self.state.lock();
        st.tokens = (st.tokens + cost).min(st.capacity);
        drop(st);
        self.notify.notify_waiters();
    }
}

struct WaitingGuard<'a> {
    gate: &'a PriorityGate,
    idx: usize,
}

impl<'a> WaitingGuard<'a> {
    fn register(gate: &'a PriorityGate, idx: usize) -> Self {
        gate.state.lock().waiting[idx] += 1;
        Self { gate, idx }
    }
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        self.gate.state.lock().waiting[self.idx] -= 1;
        self.gate.notify.notify_waiters();
    }
}

/// Held for the duration of a slot; a no-op for rate buckets.
#[derive(Debug)]
pub struct ResourceGuard {
    gate: Option<Arc<PriorityGate>>,
    cost: f64,
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        if let Some(g) = &self.gate {
            g.release(self.cost);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceConfig {
    pub gmail_units_per_minute_per_dig: f64,
    pub judge_requests_per_minute: f64,
    pub llm_requests_per_minute: f64,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            gmail_units_per_minute_per_dig: 6_000.0,
            judge_requests_per_minute: 1_200.0,
            llm_requests_per_minute: 60.0,
        }
    }
}

/// All gates, shared by every job.
pub struct Resources {
    config: RwLock<ResourceConfig>,
    gmail: Mutex<HashMap<DigId, Arc<PriorityGate>>>,
    judge: Arc<PriorityGate>,
    llm: Arc<PriorityGate>,
    sqlite: Arc<PriorityGate>,
    budget: RwLock<Arc<dyn BudgetGate>>,
}

impl Resources {
    pub fn new(config: ResourceConfig) -> Arc<Self> {
        Arc::new(Self {
            judge: Arc::new(PriorityGate::rate(config.judge_requests_per_minute)),
            llm: Arc::new(PriorityGate::rate(config.llm_requests_per_minute)),
            sqlite: Arc::new(PriorityGate::slots(1)),
            gmail: Mutex::new(HashMap::new()),
            config: RwLock::new(config),
            budget: RwLock::new(Arc::new(UnlimitedBudget)),
        })
    }

    pub fn set_budget_gate(&self, gate: Arc<dyn BudgetGate>) {
        *self.budget.write() = gate;
    }
    pub fn budget_gate(&self) -> Arc<dyn BudgetGate> {
        self.budget.read().clone()
    }

    /// Settings changed: the LLM rate is the configurable one.
    pub fn set_llm_rpm(&self, rpm: f64) {
        self.config.write().llm_requests_per_minute = rpm;
        self.llm.set_rate(rpm);
    }

    pub fn gate(&self, resource: Resource, dig: Option<&DigId>) -> Arc<PriorityGate> {
        match resource {
            Resource::GmailUnits => {
                let key = dig.cloned().unwrap_or_else(|| DigId::from("shared"));
                let per_dig = self.config.read().gmail_units_per_minute_per_dig;
                self.gmail
                    .lock()
                    .entry(key)
                    .or_insert_with(|| Arc::new(PriorityGate::rate(per_dig)))
                    .clone()
            }
            Resource::Judge => self.judge.clone(),
            Resource::Llm => self.llm.clone(),
            Resource::SqliteWriter => self.sqlite.clone(),
        }
    }

    /// The scheduler asks this before starting a budgeted job.
    pub fn budget_exhausted(
        &self,
        dig: Option<&DigId>,
        class: BudgetClass,
        priority: Priority,
    ) -> bool {
        yields_to_budget(priority) && self.budget.read().remaining(dig, class) <= 0.0
    }

    /// Acquire on behalf of a job of `priority` for `dig`.
    pub async fn acquire(
        self: &Arc<Self>,
        resource: Resource,
        dig: Option<&DigId>,
        priority: Priority,
        cost: f64,
    ) -> Result<ResourceGuard> {
        let class = match resource {
            Resource::Judge => Some(BudgetClass::Judge),
            Resource::Llm => Some(BudgetClass::Llm),
            _ => None,
        };
        if let Some(class) = class {
            if self.budget_exhausted(dig, class, priority) {
                rdebug!(
                    Category::Jobs,
                    "budget exhausted for {:?} {:?}; {:?} yields",
                    dig,
                    class,
                    priority
                );
                return Err(RiddleError::budget_exhausted(format!(
                    "daily {} budget is spent for this Dig",
                    match class {
                        BudgetClass::Judge => "judge",
                        BudgetClass::Llm => "LLM",
                    }
                )));
            }
        }
        Ok(self.gate(resource, dig).acquire(priority, cost).await)
    }
}

/// The handle a job gets: its own priority and Dig are baked in.
#[derive(Clone)]
pub struct ResourceGuards {
    pub(crate) resources: Arc<Resources>,
    pub(crate) priority: Priority,
    pub(crate) dig: Option<DigId>,
}

impl ResourceGuards {
    pub fn new(resources: Arc<Resources>, priority: Priority, dig: Option<DigId>) -> Self {
        Self {
            resources,
            priority,
            dig,
        }
    }
    pub async fn acquire(&self, resource: Resource, cost: f64) -> Result<ResourceGuard> {
        self.resources
            .acquire(resource, self.dig.as_ref(), self.priority, cost)
            .await
    }
    pub fn priority(&self) -> Priority {
        self.priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test(start_paused = true)]
    async fn a_waiting_interactive_request_preempts_backfills_next_acquisition() {
        // 60/min = 1 token per second, bucket of 60. Drain it first.
        let gate = Arc::new(PriorityGate::rate(60.0));
        drop(gate.acquire(Priority::Backfill, 60.0).await);
        assert!(gate.available() < 1.0);

        let order = Arc::new(Mutex::new(Vec::new()));
        let backfill = {
            let (g, o) = (gate.clone(), order.clone());
            tokio::spawn(async move {
                g.acquire(Priority::Backfill, 1.0).await;
                o.lock().push("backfill");
            })
        };
        tokio::task::yield_now().await;
        let interactive = {
            let (g, o) = (gate.clone(), order.clone());
            tokio::spawn(async move {
                g.acquire(Priority::Interactive, 1.0).await;
                o.lock().push("interactive");
            })
        };
        tokio::task::yield_now().await;
        assert_eq!(gate.waiting(Priority::Backfill), 1);
        assert_eq!(gate.waiting(Priority::Interactive), 1);

        interactive.await.unwrap();
        // Backfill was first in line but the interactive request took the
        // first refilled token; backfill takes the next one.
        assert_eq!(order.lock().clone(), vec!["interactive"]);
        backfill.await.unwrap();
        assert_eq!(order.lock().clone(), vec!["interactive", "backfill"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_backfill_waits_while_interactive_keeps_acquiring() {
        let gate = Arc::new(PriorityGate::rate(60.0));
        drop(gate.acquire(Priority::Backfill, 60.0).await);
        let got = Arc::new(AtomicUsize::new(0));
        let backfill = {
            let (g, got) = (gate.clone(), got.clone());
            tokio::spawn(async move {
                g.acquire(Priority::Backfill, 1.0).await;
                got.store(1, Ordering::SeqCst);
            })
        };
        tokio::task::yield_now().await;
        for _ in 0..3 {
            gate.acquire(Priority::Interactive, 1.0).await;
            assert_eq!(got.load(Ordering::SeqCst), 0, "backfill still waiting");
        }
        backfill.await.unwrap();
        assert_eq!(got.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn the_writer_slot_is_returned_on_drop() {
        let gate = Arc::new(PriorityGate::slots(1));
        let guard = gate.acquire(Priority::Backfill, 1.0).await;
        assert_eq!(gate.available(), 0.0);
        let waiter = {
            let g = gate.clone();
            tokio::spawn(async move {
                let _g = g.acquire(Priority::Interactive, 1.0).await;
            })
        };
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(guard);
        waiter.await.unwrap();
        assert_eq!(gate.available(), 1.0);
    }

    struct Exhausted;
    impl BudgetGate for Exhausted {
        fn remaining(&self, _d: Option<&DigId>, _c: BudgetClass) -> f64 {
            -0.5
        }
    }

    #[tokio::test(start_paused = true)]
    async fn an_exhausted_budget_stops_backfill_but_not_incoming() {
        let r = Resources::new(ResourceConfig::default());
        r.set_budget_gate(Arc::new(Exhausted));
        let err = r
            .acquire(Resource::Judge, None, Priority::Backfill, 1.0)
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::BudgetExhausted);
        assert!(r
            .acquire(Resource::Judge, None, Priority::Incoming, 1.0)
            .await
            .is_ok());
        assert!(
            r.acquire(
                Resource::GmailUnits,
                Some(&DigId::from("d")),
                Priority::Experiment,
                5.0
            )
            .await
            .is_ok(),
            "gmail units are not budgeted"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn gmail_units_are_per_dig() {
        let r = Resources::new(ResourceConfig {
            gmail_units_per_minute_per_dig: 10.0,
            ..Default::default()
        });
        let a = DigId::from("a");
        let b = DigId::from("b");
        r.acquire(Resource::GmailUnits, Some(&a), Priority::CatchUp, 10.0)
            .await
            .unwrap();
        assert!(r.gate(Resource::GmailUnits, Some(&a)).available() < 1.0);
        assert_eq!(r.gate(Resource::GmailUnits, Some(&b)).available(), 10.0);
    }
}
