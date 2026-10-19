//! A Sift pass's window: the batch decided together, a bounded number at a
//! time, and persisted in its original order.
//!
//! A pass decided its batch strictly in series: build the input, ask the
//! judge (two Sets of about 400 ms each for an uncached message), write,
//! next. A hundred uncached messages took eighty seconds with the judge's
//! bucket of 1,200 calls a minute almost idle, because the time went to
//! waiting on the network one message at a time (17 Oct 2026). The window
//! overlaps that waiting: decisions run through a bounded stream, and what
//! each decision needs from the store it reads for itself, so nothing is
//! shared but the `Sift` and the judge. Persisting stays serial, in the
//! window's order, so dispositions, Activity and the projection outbox
//! keep the pass's newest-first order and a thread's replies are spread
//! after its root. The helper is generic so the window's mechanics (order,
//! one failure, cancellation, the bound) are proved without a store.

use futures::{future, StreamExt};
use riddle_errors::Result;
use std::future::Future;

/// Decisions of a window in flight at once. The judge client allows 1,200
/// calls a minute, 20 a second; one stream at about 400 ms a call makes
/// two and a half a second, so six make fifteen and leave the bucket room
/// for the Director, the sender review and a second account's pass (the
/// runner allows one judge-class job per account, so two accounts make
/// twelve streams, which the client's bucket then paces). Six also bounds
/// what a 5 s judge timeout can hold up and how many escalations can pass
/// the cap's check before the first books its cost. Since a message's
/// Sets are asked together (19 Oct 2026) the judge sees up to
/// `riddle_sift::JUDGE_CALLS_IN_FLIGHT` calls at once from these streams;
/// that wrapper, not the stream count, is the bound the judge sees.
pub const JUDGE_CONCURRENCY: usize = 6;

/// Decide `items` through a stream of at most `concurrency` decisions at
/// once, and return the results in the items' order. The decisions start
/// in order and finish in any; a failed one is its own `Err` and stops
/// nothing else. `cancelled` is asked before each start: once it answers
/// true no further decision starts, those running finish, and the
/// returned vector ends with the last one started (a shorter vector means
/// the window was cut, never that a result went missing).
pub async fn decide_window<T, U, F, Fut>(
    items: Vec<T>,
    concurrency: usize,
    cancelled: impl Fn() -> bool,
    decide: F,
) -> Vec<Result<U>>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = Result<U>>,
{
    let mut slots: Vec<Option<Result<U>>> = (0..items.len()).map(|_| None).collect();
    let mut stream = futures::stream::iter(items.into_iter().enumerate())
        .take_while(|_| future::ready(!cancelled()))
        .map(|(i, item)| {
            let fut = decide(item);
            async move { (i, fut.await) }
        })
        .buffer_unordered(concurrency.max(1));
    while let Some((i, result)) = stream.next().await {
        slots[i] = Some(result);
    }
    // A cut leaves an unstarted tail; everything before it was decided.
    slots.into_iter().map_while(|s| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_errors::RiddleError;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn a_window_keeps_its_input_order_whatever_order_decisions_finish_in() {
        // Later items finish first: the result still reads 0, 1, 2, ...
        let items: Vec<usize> = (0..12).collect();
        let out = decide_window(
            items,
            4,
            || false,
            |i| async move {
                tokio::time::sleep(Duration::from_millis((12 - i as u64) * 3)).await;
                Ok(i * 10)
            },
        )
        .await;
        let got: Vec<usize> = out.into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(got, (0..12).map(|i| i * 10).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn one_failed_decision_does_not_stop_the_window() {
        let out = decide_window(
            (0..8).collect(),
            3,
            || false,
            |i: usize| async move {
                tokio::time::sleep(Duration::from_millis(2)).await;
                if i == 3 {
                    Err(RiddleError::internal("decide broke"))
                } else {
                    Ok(i)
                }
            },
        )
        .await;
        assert_eq!(out.len(), 8);
        assert_eq!(out.iter().filter(|r| r.is_err()).count(), 1);
        assert!(out[3].is_err());
        for (i, r) in out.iter().enumerate() {
            if i != 3 {
                assert_eq!(*r.as_ref().unwrap(), i);
            }
        }
    }

    #[tokio::test]
    async fn a_cancelled_window_starts_no_more_decisions_and_keeps_the_ones_made() {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let out = decide_window(
            (0..10).collect(),
            1,
            || cancel.load(Ordering::SeqCst),
            |i: usize| {
                let flag = flag.clone();
                async move {
                    if i == 2 {
                        flag.store(true, Ordering::SeqCst);
                    }
                    Ok(i)
                }
            },
        )
        .await;
        let got: Vec<usize> = out.into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(got, vec![0, 1, 2], "the third decision cancelled the rest");
    }

    #[tokio::test]
    async fn a_window_runs_at_most_the_concurrency_at_once_and_overlaps_the_waiting() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let started = Instant::now();
        let out = decide_window(
            (0..12).collect(),
            3,
            || false,
            |i: usize| {
                let in_flight = in_flight.clone();
                let peak = peak.clone();
                async move {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    Ok(i)
                }
            },
        )
        .await;
        assert_eq!(out.len(), 12);
        let peak = peak.load(Ordering::SeqCst);
        assert!(peak <= 3, "peak {peak} exceeds the bound");
        assert!(peak >= 2, "peak {peak}: the decisions did not overlap");
        // Twelve waits of 20 ms in series are 240 ms; three at once are 80.
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn one_accounts_window_stays_under_the_judge_bucket_on_its_own() {
        // A stream at 400 ms a call makes 2.5 calls a second; the client's
        // bucket is 1,200 a minute. In calls per minute: K * 150 <= 1,200.
        let per_stream_per_minute = 60_000 / 400;
        assert!(JUDGE_CONCURRENCY * per_stream_per_minute <= 1_200);
    }
}
