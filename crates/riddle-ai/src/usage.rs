//! The usage ledger hook (§56). Every model call, success or failure, becomes
//! a `UsageRow` handed to the recorder. Recording is bookkeeping and never
//! fails the call: a recorder that panics is caught and logged.

use riddle_core::{DigId, UsageRow, UsageTask};
use riddle_observability::{rerror, Category};
use std::sync::Arc;

/// The hook the integrator installs to write rows into the store.
pub type UsageRecorder = Arc<dyn Fn(UsageRow) + Send + Sync>;

/// Build and deliver a row. Panics inside the recorder are swallowed.
#[allow(clippy::too_many_arguments)]
pub fn record_usage(
    recorder: Option<&UsageRecorder>,
    provider: &str,
    model: &str,
    task: UsageTask,
    dig_id: Option<DigId>,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: Option<f64>,
    latency_ms: u64,
    ok: bool,
) {
    let Some(rec) = recorder else { return };
    let row = UsageRow {
        id: uuid::Uuid::now_v7().to_string(),
        at: chrono::Utc::now(),
        provider: provider.to_string(),
        model: model.to_string(),
        task,
        dig_id,
        trace_id: None,
        input_tokens,
        output_tokens,
        cost_usd,
        latency_ms,
        ok,
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rec(row)));
    if outcome.is_err() {
        rerror!(
            Category::Llm,
            "usage recorder panicked; row dropped (model {model}, task {})",
            task.as_str()
        );
    }
}

/// A recorder that collects rows for tests.
#[derive(Default)]
pub struct RecordingUsage(pub parking_lot::Mutex<Vec<UsageRow>>);

impl RecordingUsage {
    pub fn recorder(self: &Arc<Self>) -> UsageRecorder {
        let me = self.clone();
        Arc::new(move |row| me.0.lock().push(row))
    }
    pub fn rows(&self) -> Vec<UsageRow> {
        self.0.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_recorder_does_not_fail_the_caller() {
        let rec: UsageRecorder = Arc::new(|_| panic!("disk on fire"));
        record_usage(
            Some(&rec),
            "scripted",
            "m",
            UsageTask::Probe,
            None,
            1,
            1,
            None,
            1,
            true,
        );
    }

    #[test]
    fn rows_are_recorded_with_attribution() {
        let sink = Arc::new(RecordingUsage::default());
        let rec = sink.recorder();
        record_usage(
            Some(&rec),
            "bedrock",
            "m1",
            UsageTask::Sift,
            Some(DigId::from("d1")),
            10,
            5,
            Some(0.01),
            42,
            false,
        );
        let rows = sink.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task, UsageTask::Sift);
        assert_eq!(rows[0].dig_id.as_ref().map(|d| d.as_str()), Some("d1"));
        assert!(!rows[0].ok);
    }
}
