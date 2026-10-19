//! Retry with backoff for transient provider failures. Three attempts is the
//! right answer to a throttle; the circuit breaker (`breaker`) is the right
//! answer to an outage, and the two are kept separate on purpose (§56).

use riddle_errors::{ErrorKind, Result, RiddleError};
use std::future::Future;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            base_delay: Duration::from_millis(500),
        }
    }
}

impl RetryPolicy {
    pub fn no_delay(attempts: u32) -> Self {
        Self {
            attempts,
            base_delay: Duration::ZERO,
        }
    }

    /// Delay before attempt `n` (1-based) when the previous one failed.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::ZERO;
        }
        // 1x, 3x, 7x ... of the base: fast first retry, then real backoff.
        let factor = (1u32 << (attempt - 1)).saturating_sub(1).max(1);
        self.base_delay.saturating_mul(factor)
    }
}

/// A transient failure the policy will retry (`RiddleError::is_retryable`).
pub fn is_retryable(e: &RiddleError) -> bool {
    e.is_retryable()
}

/// Run `f` up to `policy.attempts` times, sleeping between attempts, and
/// return the last error as `LlmUnavailable` when all fail.
pub async fn with_backoff<T, F, Fut>(policy: RetryPolicy, mut f: F) -> Result<T>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last: Option<RiddleError> = None;
    for attempt in 1..=policy.attempts.max(1) {
        let delay = policy.delay_for(attempt);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        match f(attempt).await {
            Ok(v) => return Ok(v),
            Err(e) if is_retryable(&e) => {
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    let e = last.unwrap_or_else(|| RiddleError::llm_unavailable("no attempts made"));
    Err(RiddleError::new(
        ErrorKind::LlmUnavailable,
        format!("{} (after {} attempts)", e.message, policy.attempts),
    )
    .with_detail(e.detail.unwrap_or(serde_json::Value::Null)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn a_throttle_is_retried_up_to_three_times_then_reported_unavailable() {
        let calls = AtomicU32::new(0);
        let r: Result<()> = with_backoff(RetryPolicy::no_delay(3), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(RiddleError::rate_limited("throttled")) }
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let e = r.unwrap_err();
        assert_eq!(e.kind, ErrorKind::LlmUnavailable);
        assert!(e.message.contains("after 3 attempts"));
    }

    #[tokio::test]
    async fn a_non_transient_error_is_not_retried() {
        let calls = AtomicU32::new(0);
        let r: Result<()> = with_backoff(RetryPolicy::no_delay(3), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(RiddleError::policy_denied("no")) }
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(r.unwrap_err().kind, ErrorKind::PolicyDenied);
    }

    #[tokio::test]
    async fn a_success_after_a_throttle_returns_the_value() {
        let calls = AtomicU32::new(0);
        let r = with_backoff(RetryPolicy::no_delay(3), |attempt| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt < 2 {
                    Err(RiddleError::rate_limited("t"))
                } else {
                    Ok(attempt)
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(r, 2);
    }

    #[test]
    fn backoff_grows() {
        let p = RetryPolicy {
            attempts: 3,
            base_delay: Duration::from_millis(100),
        };
        assert_eq!(p.delay_for(1), Duration::ZERO);
        assert_eq!(p.delay_for(2), Duration::from_millis(100));
        assert_eq!(p.delay_for(3), Duration::from_millis(300));
    }
}
