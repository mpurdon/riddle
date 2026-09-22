//! A circuit breaker per model (§56). Three consecutive failures open the
//! circuit; after a cooldown one probe is let through (half-open) and its
//! outcome closes or re-opens the circuit. The probe slot is a guard: a probe
//! that is dropped without reporting releases the slot, so a panicking or
//! cancelled caller cannot wedge the breaker open forever.
//!
//! Retrying a single call is the right answer to a blip and the wrong answer
//! to an outage; the breaker is what turns an outage into fast `LlmUnavailable`
//! errors that Sift degrades to Unresolved instead of queueing behind.

use parking_lot::Mutex;
use riddle_errors::{Result, RiddleError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct BreakerConfig {
    pub failure_threshold: u32,
    pub cooldown: Duration,
    pub max_cooldown: Duration,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            cooldown: Duration::from_secs(30),
            max_cooldown: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Default)]
struct Circuit {
    consecutive_failures: u32,
    open_until: Option<Instant>,
    probe_in_flight: bool,
    trips: u32,
}

/// Breakers keyed by model id.
#[derive(Debug, Default)]
pub struct Breaker {
    config: BreakerConfig,
    circuits: Mutex<HashMap<String, Circuit>>,
}

/// Permission to make one call. Report the outcome with `succeed` or `fail`;
/// dropping it unreported releases a probe slot.
pub struct Permit {
    breaker: Arc<Breaker>,
    key: String,
    probe: bool,
    reported: bool,
}

impl Breaker {
    pub fn new(config: BreakerConfig) -> Self {
        Self {
            config,
            circuits: Mutex::new(HashMap::new()),
        }
    }

    pub fn state(&self, key: &str) -> BreakerState {
        let mut map = self.circuits.lock();
        let c = map.entry(key.to_string()).or_default();
        Self::state_of(c)
    }

    fn state_of(c: &Circuit) -> BreakerState {
        match c.open_until {
            Some(until) if Instant::now() < until => BreakerState::Open,
            Some(_) => BreakerState::HalfOpen,
            None => BreakerState::Closed,
        }
    }

    /// Acquire a permit or fail fast with `LlmUnavailable`.
    pub fn acquire(self: &Arc<Self>, key: &str) -> Result<Permit> {
        let mut map = self.circuits.lock();
        let c = map.entry(key.to_string()).or_default();
        match Self::state_of(c) {
            BreakerState::Closed => Ok(Permit {
                breaker: self.clone(),
                key: key.into(),
                probe: false,
                reported: false,
            }),
            BreakerState::Open => {
                let remaining = c
                    .open_until
                    .map(|u| u.saturating_duration_since(Instant::now()))
                    .unwrap_or_default();
                Err(RiddleError::llm_unavailable(format!(
                    "circuit open for {key}; retry in {}s",
                    remaining.as_secs().max(1)
                ))
                .with_detail(serde_json::json!({"retryable": false, "circuit": "open"})))
            }
            BreakerState::HalfOpen => {
                if c.probe_in_flight {
                    Err(RiddleError::llm_unavailable(format!(
                        "circuit half-open for {key}; a probe is in flight"
                    ))
                    .with_detail(serde_json::json!({"retryable": false, "circuit": "half_open"})))
                } else {
                    c.probe_in_flight = true;
                    Ok(Permit {
                        breaker: self.clone(),
                        key: key.into(),
                        probe: true,
                        reported: false,
                    })
                }
            }
        }
    }

    fn report(&self, key: &str, ok: bool, probe: bool) {
        let mut map = self.circuits.lock();
        let c = map.entry(key.to_string()).or_default();
        if probe {
            c.probe_in_flight = false;
        }
        if ok {
            c.consecutive_failures = 0;
            c.open_until = None;
            c.trips = 0;
        } else {
            c.consecutive_failures += 1;
            if probe || c.consecutive_failures >= self.config.failure_threshold {
                c.trips = c.trips.saturating_add(1);
                let factor = 1u32 << (c.trips.saturating_sub(1)).min(4);
                let cooldown = self
                    .config
                    .cooldown
                    .saturating_mul(factor)
                    .min(self.config.max_cooldown);
                c.open_until = Some(Instant::now() + cooldown);
            }
        }
    }

    fn release_probe(&self, key: &str) {
        let mut map = self.circuits.lock();
        if let Some(c) = map.get_mut(key) {
            c.probe_in_flight = false;
        }
    }

    /// For tests and the Developer pane: force the cooldown to elapse.
    pub fn expire_cooldown(&self, key: &str) {
        let mut map = self.circuits.lock();
        if let Some(c) = map.get_mut(key) {
            if c.open_until.is_some() {
                c.open_until = Some(Instant::now() - Duration::from_millis(1));
            }
        }
    }

    pub fn reset(&self, key: &str) {
        self.circuits.lock().remove(key);
    }
}

impl Permit {
    pub fn succeed(mut self) {
        self.reported = true;
        self.breaker.report(&self.key, true, self.probe);
    }
    pub fn fail(mut self) {
        self.reported = true;
        self.breaker.report(&self.key, false, self.probe);
    }
    pub fn is_probe(&self) -> bool {
        self.probe
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if !self.reported && self.probe {
            self.breaker.release_probe(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breaker() -> Arc<Breaker> {
        Arc::new(Breaker::new(BreakerConfig {
            failure_threshold: 3,
            cooldown: Duration::from_secs(60),
            max_cooldown: Duration::from_secs(60),
        }))
    }

    #[test]
    fn three_consecutive_failures_open_the_circuit_and_one_probe_closes_it() {
        let b = breaker();
        for _ in 0..3 {
            b.acquire("m").unwrap().fail();
        }
        assert_eq!(b.state("m"), BreakerState::Open);
        assert!(b.acquire("m").is_err());
        b.expire_cooldown("m");
        assert_eq!(b.state("m"), BreakerState::HalfOpen);
        let probe = b.acquire("m").unwrap();
        assert!(probe.is_probe());
        // Only one probe slot.
        assert!(b.acquire("m").is_err());
        probe.succeed();
        assert_eq!(b.state("m"), BreakerState::Closed);
        assert!(b.acquire("m").is_ok());
    }

    #[test]
    fn a_dropped_probe_releases_the_slot() {
        let b = breaker();
        for _ in 0..3 {
            b.acquire("m").unwrap().fail();
        }
        b.expire_cooldown("m");
        {
            let _probe = b.acquire("m").unwrap();
            assert!(b.acquire("m").is_err());
        }
        assert!(b.acquire("m").is_ok(), "slot released on drop");
    }

    #[test]
    fn a_failed_probe_reopens_the_circuit() {
        let b = breaker();
        for _ in 0..3 {
            b.acquire("m").unwrap().fail();
        }
        b.expire_cooldown("m");
        b.acquire("m").unwrap().fail();
        assert_eq!(b.state("m"), BreakerState::Open);
    }

    #[test]
    fn a_success_resets_the_failure_count_and_circuits_are_per_model() {
        let b = breaker();
        b.acquire("a").unwrap().fail();
        b.acquire("a").unwrap().fail();
        b.acquire("a").unwrap().succeed();
        b.acquire("a").unwrap().fail();
        assert_eq!(b.state("a"), BreakerState::Closed);
        for _ in 0..3 {
            b.acquire("b").unwrap().fail();
        }
        assert_eq!(b.state("b"), BreakerState::Open);
        assert_eq!(b.state("a"), BreakerState::Closed);
    }
}
