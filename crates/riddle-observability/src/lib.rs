//! Software logging with a fixed category vocabulary, a `Timed` guard, and a
//! ring buffer for the in-app tail (`ARCHITECTURE.md` §48).
//!
//! Two rules, both learned the hard way in earlier projects: logs carry
//! identifiers only (never subjects, bodies, addresses, names or tokens), and
//! logging never fails the work it describes. Every macro here swallows its
//! own errors. Structured payloads are appended as ` ||PAYLOAD||<json>` so the
//! log viewer can expand them without a second format.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The category vocabulary. Adding one here is a design decision; the log
/// viewer's filter reads this list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Sync,
    Projection,
    Sift,
    Judge,
    Llm,
    Mesh,
    Director,
    Expedition,
    Store,
    Jobs,
    Ui,
    Actions,
    Credentials,
    App,
}

impl Category {
    pub const ALL: [Category; 14] = [
        Category::Sync,
        Category::Projection,
        Category::Sift,
        Category::Judge,
        Category::Llm,
        Category::Mesh,
        Category::Director,
        Category::Expedition,
        Category::Store,
        Category::Jobs,
        Category::Ui,
        Category::Actions,
        Category::Credentials,
        Category::App,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Sync => "sync",
            Category::Projection => "projection",
            Category::Sift => "sift",
            Category::Judge => "judge",
            Category::Llm => "llm",
            Category::Mesh => "mesh",
            Category::Director => "director",
            Category::Expedition => "expedition",
            Category::Store => "store",
            Category::Jobs => "jobs",
            Category::Ui => "ui",
            Category::Actions => "actions",
            Category::Credentials => "credentials",
            Category::App => "app",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.as_str() == s)
    }
}

pub const PAYLOAD_MARK: &str = " ||PAYLOAD||";

/// Serialize a payload for a log line; a failure yields an empty suffix rather
/// than an error, because logging never fails the work.
pub fn payload<T: Serialize>(v: &T) -> String {
    match serde_json::to_string(v) {
        Ok(s) => format!("{PAYLOAD_MARK}{s}"),
        Err(_) => String::new(),
    }
}

/// One line of the in-memory tail.
#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub at: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub category: String,
    pub message: String,
    pub payload: Option<serde_json::Value>,
}

const RING_CAPACITY: usize = 5_000;

static RING: OnceLock<Mutex<VecDeque<LogLine>>> = OnceLock::new();

fn ring() -> &'static Mutex<VecDeque<LogLine>> {
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(RING_CAPACITY)))
}

/// Record a line in the tail. Called by the macros below; also by the log
/// plugin bridge in the desktop app so lines from `log::` land here too.
pub fn record(level: log::Level, category: &str, message: &str) {
    let (msg, payload) = match message.split_once(PAYLOAD_MARK) {
        Some((m, p)) => (m.to_string(), serde_json::from_str(p).ok()),
        None => (message.to_string(), None),
    };
    let mut r = ring().lock();
    if r.len() >= RING_CAPACITY {
        r.pop_front();
    }
    r.push_back(LogLine {
        at: chrono::Utc::now(),
        level: level.to_string(),
        category: category.to_string(),
        message: msg,
        payload,
    });
}

/// The most recent lines, newest last, optionally filtered.
pub fn tail(
    limit: usize,
    category: Option<&str>,
    min_level: Option<log::Level>,
    text: Option<&str>,
) -> Vec<LogLine> {
    let r = ring().lock();
    r.iter()
        .rev()
        .filter(|l| category.is_none_or(|c| l.category == c))
        .filter(|l| min_level.is_none_or(|m| level_rank(&l.level) <= level_rank(m.as_str())))
        .filter(|l| text.is_none_or(|t| l.message.to_lowercase().contains(&t.to_lowercase())))
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn level_rank(l: &str) -> u8 {
    match l.to_ascii_uppercase().as_str() {
        "ERROR" => 0,
        "WARN" => 1,
        "INFO" => 2,
        "DEBUG" => 3,
        _ => 4,
    }
}

#[doc(hidden)]
pub fn emit(level: log::Level, category: Category, args: std::fmt::Arguments<'_>) {
    let message = std::fmt::format(args);
    record(level, category.as_str(), &message);
    log::log!(target: category.as_str(), level, "{message}");
}

#[macro_export]
macro_rules! rlog {
    ($level:expr, $cat:expr, $($arg:tt)+) => {
        $crate::emit($level, $cat, format_args!($($arg)+))
    };
}
#[macro_export]
macro_rules! rerror { ($cat:expr, $($arg:tt)+) => { $crate::rlog!(::log::Level::Error, $cat, $($arg)+) }; }
#[macro_export]
macro_rules! rwarn { ($cat:expr, $($arg:tt)+) => { $crate::rlog!(::log::Level::Warn, $cat, $($arg)+) }; }
#[macro_export]
macro_rules! rinfo { ($cat:expr, $($arg:tt)+) => { $crate::rlog!(::log::Level::Info, $cat, $($arg)+) }; }
#[macro_export]
macro_rules! rdebug { ($cat:expr, $($arg:tt)+) => { $crate::rlog!(::log::Level::Debug, $cat, $($arg)+) }; }
#[macro_export]
macro_rules! rtrace { ($cat:expr, $($arg:tt)+) => { $crate::rlog!(::log::Level::Trace, $cat, $($arg)+) }; }

/// Warns when an operation exceeds its budget. Drop-based so early returns
/// and `?` are covered.
pub struct Timed {
    category: Category,
    label: String,
    budget: Duration,
    started: Instant,
}

impl Timed {
    pub fn start(category: Category, label: impl Into<String>, budget: Duration) -> Self {
        Self {
            category,
            label: label.into(),
            budget,
            started: Instant::now(),
        }
    }
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        let e = self.started.elapsed();
        if e > self.budget {
            rwarn!(
                self.category,
                "{} took {}ms (budget {}ms)",
                self.label,
                e.as_millis(),
                self.budget.as_millis()
            );
        } else {
            rtrace!(self.category, "{} took {}ms", self.label, e.as_millis());
        }
    }
}

/// Log where a panic happened, then run the default hook. The log gets the
/// file, line and thread only: a panic message can quote the mail being
/// processed (a slice error prints the string it was cutting). Without this
/// a crashed job said only "job handler panicked" and the location went to
/// a terminal nobody was watching.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());
        let thread = std::thread::current();
        emit(
            log::Level::Error,
            Category::App,
            format_args!(
                "panic at {at} on thread {}",
                thread.name().unwrap_or("unnamed")
            ),
        );
        default(info);
    }));
}

/// Redaction helper for identifiers that may carry content: keeps a prefix and
/// a length so two log lines can be correlated without revealing the value.
pub fn redact(s: &str) -> String {
    let mut keep = s.len().min(4);
    while !s.is_char_boundary(keep) {
        keep -= 1;
    }
    format!("{}…({})", &s[..keep], s.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn redacting_non_ascii_keeps_whole_characters() {
        assert_eq!(redact("éééé"), "éé…(8)");
    }

    use super::*;

    #[test]
    fn a_payload_is_split_from_the_message_in_the_tail() {
        record(
            log::Level::Info,
            "sift",
            &format!("classified 3{}", payload(&serde_json::json!({"dig": "d1"}))),
        );
        let lines = tail(10, Some("sift"), None, None);
        let last = lines.last().unwrap();
        assert_eq!(last.message, "classified 3");
        assert_eq!(last.payload.as_ref().unwrap()["dig"], "d1");
    }

    #[test]
    fn the_tail_filters_by_level_and_text() {
        record(log::Level::Debug, "jobs", "queued job-1");
        record(log::Level::Error, "jobs", "failed job-2");
        let errors = tail(10, Some("jobs"), Some(log::Level::Warn), None);
        assert!(errors.iter().all(|l| l.level == "ERROR"));
        let by_text = tail(10, Some("jobs"), None, Some("job-1"));
        assert!(by_text.iter().all(|l| l.message.contains("job-1")));
    }

    #[test]
    fn redaction_keeps_a_prefix_and_a_length() {
        assert_eq!(redact("secret-token"), "secr…(12)");
    }
}
