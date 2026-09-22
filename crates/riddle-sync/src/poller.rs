//! The per-Dig poll loop (§5, §53): `tokio::select!` on an interval and a
//! `Notify` trigger, exponential backoff on failure, a longer pause under
//! rate limit, and parking on an auth failure until something wakes it (a
//! sign-in, a manual refresh). The job runner calls `run_poller` once per
//! Dig; it returns when cancelled.

use crate::engine::{CancelFlag, ExternalMove, SyncEngine, SyncProgress, SyncReport};
use riddle_core::DigStatus;
use riddle_errors::{ErrorKind, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_store::repo;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

#[derive(Debug, Clone)]
pub struct PollerConfig {
    pub interval: Duration,
    pub max_backoff: Duration,
    /// Used when the provider gives no retry-after.
    pub rate_limit_pause: Duration,
    /// Bodies fetched per cycle after the incremental pass.
    pub bodies_per_cycle: u32,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(45),
            max_backoff: Duration::from_secs(15 * 60),
            rate_limit_pause: Duration::from_secs(120),
            bodies_per_cycle: 100,
        }
    }
}

pub type Hook<T> = Option<Box<dyn FnMut(T) + Send>>;
pub type RefHook<T> = Option<Box<dyn FnMut(&T) + Send>>;

/// What the poller tells the outside world. Every hook is optional.
#[derive(Default)]
pub struct PollerHooks {
    pub on_progress: Hook<SyncProgress>,
    pub on_external: Hook<ExternalMove>,
    pub on_status: Hook<DigStatus>,
    pub on_report: RefHook<SyncReport>,
    pub on_error: RefHook<RiddleError>,
}

fn retry_after(e: &RiddleError) -> Option<Duration> {
    e.detail
        .as_ref()
        .and_then(|d| d.get("retryAfterSeconds"))
        .and_then(|v| v.as_u64())
        .map(Duration::from_secs)
}

/// Run until cancelled.
pub async fn run_poller(
    engine: Arc<SyncEngine>,
    trigger: Arc<Notify>,
    cancel: CancelFlag,
    config: PollerConfig,
    mut hooks: PollerHooks,
) {
    let dig_id = engine.dig().id.clone();
    let mut delay = Duration::from_millis(10);
    let mut failures: u32 = 0;
    let mut parked = false;
    rinfo!(
        Category::Sync,
        "poller for dig {} started (interval {}s)",
        dig_id,
        config.interval.as_secs()
    );
    loop {
        if parked {
            tokio::select! {
                _ = trigger.notified() => {},
                _ = cancel.wait() => break,
            }
        } else {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {},
                _ = trigger.notified() => {},
                _ = cancel.wait() => break,
            }
        }
        if cancel.is_cancelled() {
            break;
        }
        let mut progress_hook = hooks.on_progress.take();
        let mut external_hook = hooks.on_external.take();
        let mut progress = |p: SyncProgress| {
            if let Some(h) = progress_hook.as_mut() {
                h(p);
            }
        };
        let mut external = |m: ExternalMove| {
            if let Some(h) = external_hook.as_mut() {
                h(m);
            }
        };
        let result = engine
            .sync_once(&mut progress, &cancel, &mut external)
            .await;
        let bodies = match &result {
            Ok(r) if r.complete && config.bodies_per_cycle > 0 => {
                engine
                    .fetch_bodies(config.bodies_per_cycle, &mut progress, &cancel)
                    .await
            }
            _ => Ok(0),
        };
        let (bodies_fetched, bodies_err) = match bodies {
            Ok(n) => (n, None),
            Err(e) => (0, Some(e)),
        };
        hooks.on_progress = progress_hook;
        hooks.on_external = external_hook;
        match result {
            Ok(report) => {
                failures = 0;
                parked = false;
                delay = if report.complete {
                    config.interval
                } else {
                    Duration::from_millis(50)
                };
                if let Some(h) = hooks.on_report.as_mut() {
                    h(&report);
                }
                let remaining_bodies = engine
                    .dig_store()
                    .and_then(|s| repo::messages::count_without_body(&s))
                    .unwrap_or(0);
                let status = if !report.complete {
                    DigStatus::CatchingUp {
                        remaining: 0,
                        total: 0,
                    }
                } else if remaining_bodies > 0 {
                    // Bodies still coming, but headers are current: keep polling quickly.
                    delay = Duration::from_millis(200);
                    DigStatus::CatchingUp {
                        remaining: remaining_bodies,
                        total: remaining_bodies + bodies_fetched,
                    }
                } else {
                    DigStatus::Synced
                };
                if let Some(h) = hooks.on_status.as_mut() {
                    h(status);
                }
                if let Some(e) = bodies_err {
                    rwarn!(
                        Category::Sync,
                        "dig {} body fetch failed: {}",
                        dig_id,
                        e.kind.as_str()
                    );
                }
            }
            Err(e) => {
                if let Some(h) = hooks.on_error.as_mut() {
                    h(&e);
                }
                let reason = e.kind.as_str().to_string();
                match e.kind {
                    ErrorKind::ProviderAuth | ErrorKind::ProviderAdminBlocked => {
                        parked = true;
                        rwarn!(
                            Category::Sync,
                            "poller for dig {} parked: {}",
                            dig_id,
                            reason
                        );
                    }
                    ErrorKind::ProviderRateLimited => {
                        delay = retry_after(&e)
                            .unwrap_or(config.rate_limit_pause)
                            .min(config.max_backoff);
                        rwarn!(
                            Category::Sync,
                            "poller for dig {} slowing down for {}s: rate limited",
                            dig_id,
                            delay.as_secs()
                        );
                    }
                    _ => {
                        failures = failures.saturating_add(1);
                        let factor = 2u32.saturating_pow(failures.min(10));
                        delay = config
                            .interval
                            .checked_mul(factor)
                            .unwrap_or(config.max_backoff)
                            .min(config.max_backoff);
                        rwarn!(
                            Category::Sync,
                            "poller for dig {} backing off {}s after {} failures: {}",
                            dig_id,
                            delay.as_secs(),
                            failures,
                            reason
                        );
                    }
                }
                if let Some(h) = hooks.on_status.as_mut() {
                    h(DigStatus::Degraded { reason });
                }
            }
        }
    }
    rinfo!(Category::Sync, "poller for dig {} stopped", dig_id);
}
