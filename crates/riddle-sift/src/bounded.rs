//! The bound the judge sees: calls in flight across a pass, not message
//! streams. A message's Sets are asked together (`pipeline`), so six
//! streams put twelve or eighteen calls in flight where there were six,
//! and a test saw the judge asked by eighteen at once (19 Oct 2026). The
//! wrapper counts calls, so the streams and the Sets both overlap under
//! one ceiling, which the client's rate limiter then paces.

use async_trait::async_trait;
use riddle_core::{
    JudgeAnswer, JudgeIdentity, JudgeLimits, JudgeQuestion, JudgeState, SemanticJudge,
    ServiceHealth,
};
use riddle_errors::{Result, RiddleError};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Judge calls a pass may have in flight at once: six message streams with
/// two Sets each, fully overlapped. The judge client's bucket of 1,200
/// calls a minute, at about 400 ms a call, is saturated by eight.
pub const JUDGE_CALLS_IN_FLIGHT: usize = 12;

/// A judge that lets at most `calls_in_flight` evaluations run at once and
/// answers as the judge it wraps in every other way.
pub struct BoundedJudge {
    inner: Arc<dyn SemanticJudge>,
    slots: Arc<Semaphore>,
}

impl BoundedJudge {
    pub fn new(inner: Arc<dyn SemanticJudge>, calls_in_flight: usize) -> Self {
        Self {
            inner,
            slots: Arc::new(Semaphore::new(calls_in_flight.max(1))),
        }
    }
}

#[async_trait]
impl SemanticJudge for BoundedJudge {
    async fn evaluate(
        &self,
        state: &JudgeState,
        questions: &[JudgeQuestion],
    ) -> Result<Vec<JudgeAnswer>> {
        let _permit = self
            .slots
            .acquire()
            .await
            .map_err(|_| RiddleError::judge_unavailable("judge slots closed"))?;
        self.inner.evaluate(state, questions).await
    }
    fn limits(&self) -> JudgeLimits {
        self.inner.limits()
    }
    fn identity(&self) -> JudgeIdentity {
        self.inner.identity()
    }
    fn health(&self) -> ServiceHealth {
        self.inner.health()
    }
    async fn probe(&self) -> Result<u64> {
        self.inner.probe().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_judge::ScriptedJudge;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting {
        inner: ScriptedJudge,
        in_flight: AtomicUsize,
        peak: AtomicUsize,
    }

    #[async_trait]
    impl SemanticJudge for Counting {
        async fn evaluate(
            &self,
            state: &JudgeState,
            questions: &[JudgeQuestion],
        ) -> Result<Vec<JudgeAnswer>> {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let out = self.inner.evaluate(state, questions).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            out
        }
        fn limits(&self) -> JudgeLimits {
            self.inner.limits()
        }
        fn identity(&self) -> JudgeIdentity {
            self.inner.identity()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn at_most_the_bound_is_in_flight_however_many_ask_at_once() {
        let counting = Arc::new(Counting {
            inner: ScriptedJudge::new(),
            in_flight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        });
        let judge: Arc<dyn SemanticJudge> = Arc::new(BoundedJudge::new(counting.clone(), 3));
        let state = JudgeState::new("hello".into());
        let calls = (0..10).map(|_| {
            let (j, s) = (judge.clone(), state.clone());
            async move { j.evaluate(&s, &[]).await }
        });
        for r in futures::future::join_all(calls).await {
            r.unwrap();
        }
        assert!(
            counting.peak.load(Ordering::SeqCst) <= 3,
            "peak {}",
            counting.peak.load(Ordering::SeqCst)
        );
        assert!(
            counting.peak.load(Ordering::SeqCst) >= 2,
            "calls overlapped"
        );
    }
}
