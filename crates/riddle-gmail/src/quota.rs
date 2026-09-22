//! A per-user quota limiter: Gmail allows 6,000 units per user per minute
//! (§16) and answers `rateLimitExceeded` when exceeded. Spending units here
//! before each call keeps sync inside the budget instead of learning about
//! it from 403s; the 403 path still exists for the shared project quota.

use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub const UNITS_PER_MINUTE: u32 = 6_000;

pub mod cost {
    pub const PROFILE: u32 = 1;
    pub const LABELS_LIST: u32 = 1;
    pub const LABELS_CREATE: u32 = 5;
    pub const MESSAGES_LIST: u32 = 5;
    pub const MESSAGES_GET: u32 = 20;
    pub const HISTORY_LIST: u32 = 2;
    pub const BATCH_MODIFY: u32 = 50;
    pub const SEND: u32 = 100;
    pub const DRAFT_CREATE: u32 = 10;
    pub const DRAFT_UPDATE: u32 = 15;
    pub const DRAFT_DELETE: u32 = 10;
    pub const TRASH: u32 = 5;
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// A token bucket refilled continuously at the per-minute rate.
pub struct QuotaLimiter {
    capacity: f64,
    per_second: f64,
    bucket: Mutex<Bucket>,
}

impl Default for QuotaLimiter {
    fn default() -> Self {
        Self::new(UNITS_PER_MINUTE)
    }
}

impl QuotaLimiter {
    pub fn new(units_per_minute: u32) -> Self {
        Self {
            capacity: units_per_minute as f64,
            per_second: units_per_minute as f64 / 60.0,
            bucket: Mutex::new(Bucket {
                tokens: units_per_minute as f64,
                last: Instant::now(),
            }),
        }
    }

    fn refill(&self, b: &mut Bucket) {
        let now = Instant::now();
        let elapsed = now.duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * self.per_second).min(self.capacity);
        b.last = now;
    }

    /// How long until `units` are available; zero when they are now.
    pub fn wait_for(&self, units: u32) -> Duration {
        let mut b = self.bucket.lock();
        self.refill(&mut b);
        let need = units as f64;
        if b.tokens >= need {
            b.tokens -= need;
            Duration::ZERO
        } else {
            let deficit = need - b.tokens;
            b.tokens -= need; // goes negative: the caller has committed to wait
            Duration::from_secs_f64(deficit / self.per_second)
        }
    }

    /// Spend `units`, sleeping first when the bucket is short.
    pub async fn acquire(&self, units: u32) {
        let wait = self.wait_for(units);
        if !wait.is_zero() {
            riddle_observability::rdebug!(
                riddle_observability::Category::Sync,
                "quota: waiting {}ms for {} units",
                wait.as_millis(),
                units
            );
            tokio::time::sleep(wait).await;
        }
    }

    pub fn available(&self) -> f64 {
        let mut b = self.bucket.lock();
        self.refill(&mut b);
        b.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bucket_spends_then_asks_for_a_wait_proportional_to_the_deficit() {
        let q = QuotaLimiter::new(600); // 10 units per second
        assert_eq!(q.wait_for(500), Duration::ZERO);
        let wait = q.wait_for(200);
        assert!(
            wait.as_secs_f64() > 9.0 && wait.as_secs_f64() <= 10.1,
            "100 short at 10/s is about 10s: {wait:?}"
        );
        assert!(q.available() < 0.0 || q.available() < 1.0);
    }
}
