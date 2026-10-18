//! Software logging with a fixed category vocabulary, a `Timed` guard, and a
//! ring buffer for the in-app tail (`ARCHITECTURE.md` §48).
//!
//! Two rules, both learned the hard way in earlier projects: logs carry
//! identifiers only (never subjects, bodies, addresses, names or tokens), and
//! logging never fails the work it describes. Every macro here swallows its
//! own errors. Structured payloads are appended as ` ||PAYLOAD||<json>` so the
//! log viewer can expand them without a second format.
//!
//! Bookkeeping that fails is swallowed by design (an Activity row, a usage
//! row, a cache write), and a swallowed failure used to leave one warn line
//! in a ring buffer of five thousand and nothing else: twenty-five such sites
//! had no count, so a usage row failing on every call looked like one bad
//! moment (17 Oct 2026). `swallowed!` logs the line and counts it in a table
//! keyed by (category, site) that `dev_info` and the diagnostics export read.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
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

impl LogLine {
    /// Whether the line passes the tail's filters. The text filter reads the
    /// payload too, so a pass id or a trace id, which ride in the payload
    /// and not in the message, finds the lines of one pass.
    fn passes(
        &self,
        category: Option<&str>,
        min_level: Option<log::Level>,
        text: Option<&str>,
    ) -> bool {
        category.is_none_or(|c| self.category == c)
            && min_level.is_none_or(|m| level_rank(&self.level) <= level_rank(m.as_str()))
            && text.is_none_or(|t| {
                let t = t.to_lowercase();
                self.message.to_lowercase().contains(&t)
                    || self
                        .payload
                        .as_ref()
                        .is_some_and(|p| p.to_string().to_lowercase().contains(&t))
            })
    }
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
        .filter(|l| l.passes(category, min_level, text))
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// How much of the log file the file tail reads, from the end.
pub const FILE_TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Parse one line of the log file as the desktop writes it,
/// `[rfc3339][LEVEL][target] message ||PAYLOAD||json`; `None` for a line in
/// any other shape (a continuation, a torn first line).
pub fn parse_file_line(line: &str) -> Option<LogLine> {
    let rest = line.strip_prefix('[')?;
    let (at, rest) = rest.split_once("][")?;
    let (level, rest) = rest.split_once("][")?;
    let (category, message) = rest.split_once("] ")?;
    let at = at.parse::<chrono::DateTime<chrono::Utc>>().ok()?;
    let (message, payload) = match message.split_once(PAYLOAD_MARK) {
        Some((m, p)) => (m.to_string(), serde_json::from_str(p).ok()),
        None => (message.to_string(), None),
    };
    Some(LogLine {
        at,
        level: level.to_string(),
        category: category.to_string(),
        message,
        payload,
    })
}

/// The newest lines of a log file, oldest first: the file is read from
/// its end, at most `max_bytes`, never whole (`APPLICATION_FEATURES.md`
/// §5, "In-app tail"). A missing or unreadable file is an error for the
/// caller to swallow, since logs never fail the work.
pub fn tail_file(
    path: &std::path::Path,
    limit: usize,
    max_bytes: u64,
    category: Option<&str>,
    min_level: Option<log::Level>,
    text: Option<&str>,
) -> std::io::Result<Vec<LogLine>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    f.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((len - start) as usize);
    f.read_to_end(&mut bytes)?;
    let body = String::from_utf8_lossy(&bytes);
    let mut lines = body.lines();
    if start > 0 {
        // The first line is cut where the window began.
        lines.next();
    }
    let kept: Vec<LogLine> = lines
        .rev()
        .filter_map(parse_file_line)
        .filter(|l| l.passes(category, min_level, text))
        .take(limit)
        .collect();
    Ok(kept.into_iter().rev().collect())
}

/// The ring's tail, and when it holds fewer lines than asked (the app just
/// started; the ring is five thousand lines long), the lines before them
/// from the log file, merged by time. Lines from the file older than the
/// ring's oldest line are added; the rest are the ring's own lines written
/// to disk, so they are not repeated. Before this, the Logs pane was empty
/// after every restart although the file had the night's warnings.
pub fn tail_with_file(
    limit: usize,
    category: Option<&str>,
    min_level: Option<log::Level>,
    text: Option<&str>,
    file: Option<&std::path::Path>,
) -> Vec<LogLine> {
    let mut lines = tail(limit, category, min_level, text);
    if lines.len() >= limit {
        return lines;
    }
    let Some(path) = file else {
        return lines;
    };
    let oldest = {
        let r = ring().lock();
        r.front().map(|l| l.at)
    };
    let want = limit - lines.len();
    match tail_file(path, limit, FILE_TAIL_BYTES, category, min_level, text) {
        Ok(from_file) => {
            let mut older: Vec<LogLine> = from_file
                .into_iter()
                .filter(|l| oldest.is_none_or(|o| l.at < o))
                .collect();
            if older.len() > want {
                older.drain(..older.len() - want);
            }
            older.append(&mut lines);
            older
        }
        Err(_) => lines,
    }
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

/// A failure that was logged and dropped so the work it described could go
/// on: `swallowed!(Category::Sift, "sift_activity", err)` logs at warn and
/// counts under (category, site); a trailing format gives the log line its
/// identifiers (`swallowed!(cat, "retry_stale", err, "for dig {}", dig.id)`).
/// The error's `Display` must carry identifiers only; `RiddleError`'s does.
#[macro_export]
macro_rules! swallowed {
    ($cat:expr, $site:expr, $err:expr) => {
        $crate::swallow($cat, $site, &$err, ::std::option::Option::None)
    };
    ($cat:expr, $site:expr, $err:expr, $($arg:tt)+) => {
        $crate::swallow(
            $cat,
            $site,
            &$err,
            ::std::option::Option::Some(format_args!($($arg)+)),
        )
    };
}

/// One row of the swallowed-failure table: how often a site dropped a
/// failure, when it last did, and what the last one said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwallowedFailure {
    pub category: String,
    pub site: String,
    pub count: u64,
    pub last_at: chrono::DateTime<chrono::Utc>,
    pub last_error: String,
}

type SwallowedKey = (&'static str, &'static str);

static SWALLOWED: OnceLock<Mutex<BTreeMap<SwallowedKey, SwallowedFailure>>> = OnceLock::new();

fn swallowed() -> &'static Mutex<BTreeMap<SwallowedKey, SwallowedFailure>> {
    SWALLOWED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[doc(hidden)]
pub fn swallow(
    category: Category,
    site: &'static str,
    err: &dyn std::fmt::Display,
    context: Option<std::fmt::Arguments<'_>>,
) {
    let text = err.to_string();
    match context {
        Some(c) => emit(log::Level::Warn, category, format_args!("{c}: {text}")),
        None => emit(log::Level::Warn, category, format_args!("{site}: {text}")),
    }
    let mut table = swallowed().lock();
    let row = table
        .entry((category.as_str(), site))
        .or_insert_with(|| SwallowedFailure {
            category: category.as_str().to_string(),
            site: site.to_string(),
            count: 0,
            last_at: chrono::Utc::now(),
            last_error: String::new(),
        });
    row.count += 1;
    row.last_at = chrono::Utc::now();
    row.last_error = text;
}

/// Every site that has swallowed a failure since launch, by category then
/// site. Read by `dev_info`, the Failures pane and the diagnostics export.
pub fn swallowed_failures() -> Vec<SwallowedFailure> {
    swallowed().lock().values().cloned().collect()
}

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

    #[test]
    fn the_file_tail_returns_the_newest_lines_of_a_log_file_and_merges_before_the_ring() {
        let dir = std::env::temp_dir().join(format!("riddle-logtail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("riddle-test.log");
        let mut body = String::new();
        for i in 0..5 {
            let extra = if i == 4 {
                payload(&serde_json::json!({"passId": "pass-x"}))
            } else {
                String::new()
            };
            body.push_str(&format!(
                "[2020-01-01T00:00:0{i}.000Z][INFO][filetail] line {i}{extra}\n"
            ));
        }
        body.push_str("not a log line\n");
        std::fs::write(&path, &body).unwrap();

        let newest = tail_file(&path, 2, 1 << 20, None, None, None).unwrap();
        let msgs: Vec<&str> = newest.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(msgs, ["line 3", "line 4"]);
        assert_eq!(newest[1].category, "filetail");
        assert_eq!(newest[1].level, "INFO");
        assert_eq!(newest[1].payload.as_ref().unwrap()["passId"], "pass-x");
        // The filters read the file as they read the ring; text reads the payload too.
        let by_pass =
            tail_file(&path, 10, 1 << 20, Some("filetail"), None, Some("pass-x")).unwrap();
        assert_eq!(by_pass.len(), 1);
        assert!(
            tail_file(&path, 10, 1 << 20, None, Some(log::Level::Warn), None)
                .unwrap()
                .is_empty()
        );
        // A window that starts mid-file drops the torn first line and keeps the rest.
        let torn = tail_file(&path, 10, 200, None, None, None).unwrap();
        assert!(!torn.is_empty() && torn.len() < 5, "{torn:?}");
        assert_eq!(torn.last().unwrap().message, "line 4");
        assert_ne!(torn.first().unwrap().message, "line 0");

        // Short of lines, the ring is preceded by the file's older ones, once each.
        record(log::Level::Info, "filetail", "ring line");
        let merged = tail_with_file(10, Some("filetail"), None, None, Some(&path));
        let msgs: Vec<&str> = merged.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(
            msgs,
            [
                "line 0",
                "line 1",
                "line 2",
                "line 3",
                "line 4",
                "ring line"
            ]
        );
        // Enough in the ring: the file is not needed.
        let ring_only = tail_with_file(1, Some("filetail"), None, None, Some(&path));
        assert_eq!(ring_only.len(), 1);
        assert_eq!(ring_only[0].message, "ring line");
        // No file, or an unreadable one: the ring alone, never an error.
        assert_eq!(
            tail_with_file(
                10,
                Some("filetail"),
                None,
                None,
                Some(&dir.join("missing.log"))
            )
            .len(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_swallowed_failure_is_counted_under_its_site_and_logged_at_warn() {
        let err = std::io::Error::other("disk full");
        swallowed!(Category::Store, "test_site_a", err);
        swallowed!(Category::Store, "test_site_a", err, "for dig {}", "d1");
        let rows = swallowed_failures();
        let row = rows
            .iter()
            .find(|r| r.site == "test_site_a")
            .expect("the site is in the table");
        assert_eq!(row.category, "store");
        assert_eq!(row.count, 2);
        assert_eq!(row.last_error, "disk full");
        let logged = tail(
            50,
            Some("store"),
            Some(log::Level::Warn),
            Some("for dig d1"),
        );
        assert!(logged
            .iter()
            .any(|l| l.level == "WARN" && l.message == "for dig d1: disk full"));
    }
}
