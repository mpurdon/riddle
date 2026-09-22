//! A circuit breaker for the judge (§56). After N consecutive failures it
//! opens and every caller gets `JudgeUnavailable` immediately, which Sift
//! turns into Unresolved; after a cooldown exactly one probe is admitted.
//! The incident behind the single probe slot: a half-open breaker that let
//! every waiting job retry at once, which is a thundering herd against a
//! vendor that is already struggling.

use parking_lot::Mutex;
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Allow,
    /// Admitted as the one probe of an open breaker.
    Probe,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BreakerState {
    Closed { consecutive_failures: u32 },
    Open { open_for_ms: u64, probing: bool },
}

struct Inner {
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    probe_in_flight: bool,
}

pub struct CircuitBreaker {
    inner: Mutex<Inner>,
    threshold: u32,
    cooldown: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner {
                consecutive_failures: 0,
                opened_at: None,
                probe_in_flight: false,
            }),
            threshold: threshold.max(1),
            cooldown,
        }
    }

    pub fn admit(&self) -> Admission {
        let mut i = self.inner.lock();
        match i.opened_at {
            None => Admission::Allow,
            Some(at) => {
                if i.probe_in_flight || at.elapsed() < self.cooldown {
                    Admission::Reject
                } else {
                    i.probe_in_flight = true;
                    Admission::Probe
                }
            }
        }
    }

    pub fn record_success(&self) {
        let mut i = self.inner.lock();
        i.consecutive_failures = 0;
        i.opened_at = None;
        i.probe_in_flight = false;
    }

    pub fn record_failure(&self) {
        let mut i = self.inner.lock();
        i.consecutive_failures += 1;
        i.probe_in_flight = false;
        if i.consecutive_failures >= self.threshold {
            i.opened_at = Some(Instant::now());
        }
    }

    /// A probe that never completed (cancelled task) must free the slot.
    pub fn abandon_probe(&self) {
        let mut i = self.inner.lock();
        i.probe_in_flight = false;
    }

    pub fn is_open(&self) -> bool {
        self.inner.lock().opened_at.is_some()
    }

    pub fn state(&self) -> BreakerState {
        let i = self.inner.lock();
        match i.opened_at {
            None => BreakerState::Closed {
                consecutive_failures: i.consecutive_failures,
            },
            Some(at) => BreakerState::Open {
                open_for_ms: at.elapsed().as_millis() as u64,
                probing: i.probe_in_flight,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_breaker_opens_after_the_threshold_admits_one_probe_and_closes_on_success() {
        let b = CircuitBreaker::new(3, Duration::ZERO);
        assert_eq!(b.admit(), Admission::Allow);
        b.record_failure();
        b.record_failure();
        assert!(!b.is_open());
        b.record_failure();
        assert!(b.is_open());
        assert_eq!(b.admit(), Admission::Probe);
        assert_eq!(b.admit(), Admission::Reject, "only one probe at a time");
        b.record_failure();
        assert!(b.is_open());
        assert_eq!(b.admit(), Admission::Probe);
        b.record_success();
        assert!(!b.is_open());
        assert_eq!(b.admit(), Admission::Allow);
    }

    #[test]
    fn an_open_breaker_rejects_until_the_cooldown_has_passed() {
        let b = CircuitBreaker::new(1, Duration::from_secs(60));
        b.record_failure();
        assert_eq!(b.admit(), Admission::Reject);
        assert!(matches!(
            b.state(),
            BreakerState::Open { probing: false, .. }
        ));
    }
}
