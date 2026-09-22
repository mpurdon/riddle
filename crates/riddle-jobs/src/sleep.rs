//! Sleep detection (§5): a 30 s tick compares the wall clock with the
//! monotonic clock; a gap over 90 s means the machine slept. On wake every
//! job is paused and immediately resumed, which makes running handlers
//! yield and restart from their checkpoints on fresh connections, and the
//! `WakeHook` fires so the integrator runs a refresh pass. Window focus
//! triggers the same refresh. The rationale is Sleipnir's: the monotonic
//! clock stops during sleep on macOS, so the wall-clock gap is the signal.

use crate::queue::JobQueue;
use crate::spec::PauseReason;
use parking_lot::Mutex;
use riddle_core::Clock;
use riddle_observability::{rinfo, Category};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TICK: Duration = Duration::from_secs(30);
pub const SLEEP_GAP: Duration = Duration::from_secs(90);

/// Called after a wake or a focus; the integrator triggers a refresh.
pub trait WakeHook: Send + Sync {
    fn on_wake(&self, reason: WakeReason);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    /// The machine slept for about this long.
    Sleep(Duration),
    Focus,
}

pub struct SleepDetector {
    queue: Arc<JobQueue>,
    clock: Arc<dyn Clock>,
    hook: Arc<dyn WakeHook>,
    last: Mutex<(chrono::DateTime<chrono::Utc>, Instant)>,
    last_focus_refresh: Mutex<Option<Instant>>,
}

impl SleepDetector {
    pub fn new(queue: Arc<JobQueue>, clock: Arc<dyn Clock>, hook: Arc<dyn WakeHook>) -> Arc<Self> {
        let now = (clock.now(), Instant::now());
        Arc::new(Self {
            queue,
            clock,
            hook,
            last: Mutex::new(now),
            last_focus_refresh: Mutex::new(None),
        })
    }

    /// One tick: returns the sleep gap when one was detected.
    pub fn check(&self) -> Option<Duration> {
        let wall = self.clock.now();
        let mono = Instant::now();
        let (prev_wall, prev_mono) = std::mem::replace(&mut *self.last.lock(), (wall, mono));
        let wall_gap = (wall - prev_wall).to_std().unwrap_or_default();
        let mono_gap = mono.duration_since(prev_mono);
        let gap = wall_gap.saturating_sub(mono_gap);
        if gap > SLEEP_GAP {
            rinfo!(Category::Jobs, "resumed after {}s asleep", gap.as_secs());
            self.queue.pause_all(PauseReason::Sleep);
            self.queue.resume_all();
            self.hook.on_wake(WakeReason::Sleep(gap));
            Some(gap)
        } else {
            None
        }
    }

    /// A window focus or visibility change: refresh, at most once a minute.
    pub fn on_focus(&self) {
        let mut last = self.last_focus_refresh.lock();
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return;
        }
        *last = Some(Instant::now());
        drop(last);
        self.hook.on_wake(WakeReason::Focus);
    }

    pub fn start(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                me.check();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{FixedClock, RecordingSink};
    use riddle_store::{Store, StoreKind};

    #[derive(Default)]
    struct Hook(Mutex<Vec<WakeReason>>);
    impl WakeHook for Hook {
        fn on_wake(&self, r: WakeReason) {
            self.0.lock().push(r);
        }
    }

    #[tokio::test]
    async fn a_wall_clock_gap_over_ninety_seconds_pauses_resumes_and_wakes() {
        let clock = Arc::new(FixedClock::at(chrono::Utc::now()));
        let queue = JobQueue::open(
            Arc::new(Store::in_memory(StoreKind::Index).unwrap()),
            clock.clone(),
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
        let hook = Arc::new(Hook::default());
        let detector = SleepDetector::new(queue.clone(), clock.clone(), hook.clone());
        clock.advance(chrono::Duration::seconds(30));
        assert_eq!(detector.check(), None, "a normal tick");
        clock.advance(chrono::Duration::minutes(5));
        let gap = detector.check().expect("sleep detected");
        assert!(gap >= Duration::from_secs(299));
        assert!(!queue.is_paused(), "resumed immediately");
        assert!(matches!(hook.0.lock().as_slice(), [WakeReason::Sleep(_)]));
    }

    #[tokio::test]
    async fn focus_refreshes_at_most_once_a_minute() {
        let clock = Arc::new(FixedClock::at(chrono::Utc::now()));
        let queue = JobQueue::open(
            Arc::new(Store::in_memory(StoreKind::Index).unwrap()),
            clock.clone(),
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
        let hook = Arc::new(Hook::default());
        let detector = SleepDetector::new(queue, clock, hook.clone());
        detector.on_focus();
        detector.on_focus();
        assert_eq!(hook.0.lock().len(), 1);
    }
}
