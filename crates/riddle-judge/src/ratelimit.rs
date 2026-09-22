//! A token bucket for the judge's 1,200 requests per minute (§55), shared by
//! every Dig. The bucket's burst capacity is a fraction of the minute so a
//! backfill cannot fire the whole minute's allowance in one second and trip
//! the vendor's tokens-per-second limit.

use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub struct TokenBucket {
    state: Mutex<(f64, Instant)>,
    capacity: f64,
    refill_per_sec: f64,
}

impl TokenBucket {
    pub fn per_minute(rpm: u32) -> Self {
        let capacity = (rpm as f64 / 4.0).max(1.0);
        Self {
            state: Mutex::new((capacity, Instant::now())),
            capacity,
            refill_per_sec: rpm as f64 / 60.0,
        }
    }

    fn refill(&self, s: &mut (f64, Instant)) {
        let now = Instant::now();
        let elapsed = now.duration_since(s.1).as_secs_f64();
        s.0 = (s.0 + elapsed * self.refill_per_sec).min(self.capacity);
        s.1 = now;
    }

    /// Take a token now, or say how long until one is available.
    pub fn try_acquire(&self) -> Result<(), Duration> {
        let mut s = self.state.lock();
        self.refill(&mut s);
        if s.0 >= 1.0 {
            s.0 -= 1.0;
            Ok(())
        } else {
            let missing = 1.0 - s.0;
            Err(Duration::from_secs_f64(missing / self.refill_per_sec))
        }
    }

    /// Wait for a token.
    pub async fn acquire(&self) {
        loop {
            match self.try_acquire() {
                Ok(()) => return,
                Err(wait) => tokio::time::sleep(wait.max(Duration::from_millis(1))).await,
            }
        }
    }

    pub fn available(&self) -> f64 {
        let mut s = self.state.lock();
        self.refill(&mut s);
        s.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bucket_refuses_once_its_burst_is_spent_and_reports_the_wait() {
        let b = TokenBucket::per_minute(60);
        // capacity 15
        for _ in 0..15 {
            assert!(b.try_acquire().is_ok());
        }
        let wait = b.try_acquire().unwrap_err();
        assert!(wait <= Duration::from_secs(1));
        assert!(wait > Duration::ZERO);
    }
}
