//! A pass decides its window in parallel and records it in order. A
//! hundred uncached messages took eighty seconds in series, two Sets of
//! about 400 ms each, while the judge's bucket of 1,200 calls a minute sat
//! almost idle (17 Oct 2026). These tests run a window against a judge
//! that waits like a network call and prove the waiting overlaps, every
//! message still gets exactly one disposition and one trace, the
//! dispositions are recorded newest first as the window was read, and the
//! pass stamps one id on its work.

use riddle_app::window::JUDGE_CONCURRENCY;
use riddle_core::*;
use riddle_judge::ScriptedJudge;
use riddle_sift::JUDGE_CALLS_IN_FLIGHT;
use riddle_store::repo;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

mod common;

/// The scripted judge made slow: every call first waits as a network call
/// would, `base` plus a jitter that varies call by call so decisions
/// finish in another order than they started. The most calls in flight at
/// once is recorded, which is what proves the overlap.
struct SlowJudge {
    inner: ScriptedJudge,
    base: Duration,
    jitter: bool,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
}

impl SlowJudge {
    fn new(base: Duration, jitter: bool) -> Self {
        Self {
            inner: ScriptedJudge::new(),
            base,
            jitter,
            in_flight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }
    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl SemanticJudge for SlowJudge {
    async fn evaluate(
        &self,
        state: &JudgeState,
        questions: &[JudgeQuestion],
    ) -> riddle_errors::Result<Vec<JudgeAnswer>> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        let call = self.inner.calls();
        let jitter = if self.jitter {
            Duration::from_millis(((call * 7) % 5) as u64 * 10)
        } else {
            Duration::ZERO
        };
        tokio::time::sleep(self.base + jitter).await;
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

/// `n` new, unclassified messages from `n` strangers, each a question that
/// needs the judge, a second apart so the pass reads them newest first:
/// the first id returned is the newest.
fn fresh_messages(app: &riddle_app::App, dig: &Dig, tag: &str, n: usize) -> Vec<MessageId> {
    let store = app.stores.dig(&dig.id).unwrap();
    let base = chrono::Utc::now();
    (0..n)
        .map(|i| {
            let mut m = riddle_mail::fixture::blank_message(dig.id.as_str(), &format!("{tag}-{i}"));
            let name = format!("Person {i}");
            m.from = Address::new(format!("person{i}@partner{i}.example"), Some(&name));
            m.to = vec![Address::new(dig.email.clone(), None)];
            m.subject = format!("Question about draft {i}");
            m.body_text = Some(format!(
                "Hi, could you look at draft {i} and let me know what you think? Thanks, Person {i}"
            ));
            m.snippet = format!("Hi, could you look at draft {i}");
            m.has_body = true;
            m.date = base - chrono::Duration::seconds(i as i64);
            repo::messages::upsert(&store, &m).unwrap();
            m.id
        })
        .collect()
}

/// The non-shadow dispositions of the Dig in the order they were written.
fn recorded_order(store: &riddle_store::Store) -> Vec<String> {
    store
        .read(|c| {
            let mut st = c.prepare(
                "SELECT message_id FROM classifications WHERE shadow = 0 ORDER BY rowid",
            )?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect();
            rows
        })
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_of_slow_judge_calls_finishes_in_parallel_with_one_disposition_and_one_trace_each()
{
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    let ids = fresh_messages(&app, &dig, "win", 24);
    let judge = Arc::new(SlowJudge::new(Duration::from_millis(50), false));
    *app.services.judge.write() = Some(judge.clone());

    let report = app.sift_pass(&dig, 100, None).await.unwrap();

    assert_eq!(report.processed, 24, "{report:?}");
    assert_eq!(report.failed, 0, "{report:?}");
    assert_eq!(report.window, 24, "{report:?}");
    assert_eq!(report.concurrency, JUDGE_CONCURRENCY as u64);
    let calls = judge.inner.calls();
    assert!(
        calls >= 24,
        "every message asked the judge at least once: {calls} calls"
    );
    // In series the pass could not finish before every wait had passed.
    let serial_floor_ms = calls as u64 * 50;
    // A message's Sets are asked together, so its judge time is the longer
    // Set's wait, not their sum (19 Oct 2026): the summed judge time still
    // counts one wait per message.
    let per_message_floor_ms = 24 * 50;
    assert!(
        report.judge_ms >= per_message_floor_ms * 9 / 10,
        "summed judge time counts a wait per message: {calls} calls, {report:?}"
    );
    assert!(
        report.elapsed_ms < serial_floor_ms / 2,
        "the window overlapped its waits: {calls} calls would take {serial_floor_ms}ms in series, {report:?}"
    );
    let peak = judge.peak();
    assert!(peak >= 2, "calls overlapped: peak {peak}");
    // A message's Sets are asked together, so the judge sees more calls at
    // once than there are streams; the bound it sees is calls in flight.
    assert!(peak <= JUDGE_CALLS_IN_FLIGHT, "the bound held: peak {peak}");

    let store = app.stores.dig(&dig.id).unwrap();
    for id in &ids {
        let dispositions = repo::classifications::history(&store, id)
            .unwrap()
            .into_iter()
            .filter(|k| !k.shadow)
            .count();
        assert_eq!(dispositions, 1, "one disposition for {id}");
        let traces = repo::traces::for_message(&store, id).unwrap().len();
        assert_eq!(traces, 1, "one trace for {id}");
        assert!(
            repo::classifications::current(&store, id)
                .unwrap()
                .is_some(),
            "{id} is decided"
        );
    }
    assert_eq!(repo::messages::unclassified_count(&store).unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispositions_are_recorded_in_the_windows_newest_first_order() {
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    let ids = fresh_messages(&app, &dig, "order", 24);
    // Jitter makes decisions finish out of the order they started in.
    let judge = Arc::new(SlowJudge::new(Duration::from_millis(10), true));
    *app.services.judge.write() = Some(judge.clone());

    let report = app.sift_pass(&dig, 100, None).await.unwrap();

    assert_eq!(report.processed, 24, "{report:?}");
    assert!(judge.peak() >= 2, "calls overlapped: peak {}", judge.peak());
    let store = app.stores.dig(&dig.id).unwrap();
    let ours: Vec<String> = recorded_order(&store)
        .into_iter()
        .filter(|m| ids.iter().any(|i| i.as_str() == m))
        .collect();
    let expected: Vec<String> = ids.iter().map(|i| i.as_str().to_string()).collect();
    assert_eq!(
        ours, expected,
        "recorded newest first, as the window was read"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pass_stamps_one_id_on_its_work_and_the_next_pass_another() {
    let (app, ops) = common::boot_scripted(false, |s| s.shadow_period_days = 0).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Work", "me@work.example", 3, 8, json!({})).await;
    fresh_messages(&app, &dig, "first", 2);
    let first = app.sift_pass(&dig, 100, None).await.unwrap();
    fresh_messages(&app, &dig, "second", 3);
    let second = app.sift_pass(&dig, 100, None).await.unwrap();

    assert!(
        uuid::Uuid::parse_str(&first.pass_id).is_ok(),
        "{}",
        first.pass_id
    );
    assert!(
        uuid::Uuid::parse_str(&second.pass_id).is_ok(),
        "{}",
        second.pass_id
    );
    assert_ne!(first.pass_id, second.pass_id);
    assert_eq!((first.window, second.window), (2, 3));
    let line = riddle_observability::tail(
        500,
        Some("sift"),
        Some(log::Level::Info),
        Some(&format!("pass {}:", dig.id)),
    )
    .pop()
    .expect("the pass logged a line");
    assert!(
        line.message.contains(&format!(
            "window 3 concurrency {JUDGE_CONCURRENCY}, pass id {}",
            second.pass_id
        )),
        "{}",
        line.message
    );
}
