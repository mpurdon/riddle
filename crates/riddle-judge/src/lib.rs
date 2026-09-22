//! The semantic judge implementations behind `riddle_core::SemanticJudge`
//! (ARCHITECTURE.md §18, §20). `JevJudge` talks to TypeSafe AI's System One
//! endpoint with a pinned model id, a rate limiter shared by every Dig, and a
//! circuit breaker that answers `JudgeUnavailable` at once when the vendor is
//! down, because the fallback (Unresolved, nothing hidden) is cheap and a
//! Sift that waits five seconds per message for a dead endpoint is not.
//! `NullJudge` and `ScriptedJudge` are the doubles tests and Demo Mode use.
//!
//! The key never appears in a Debug print, a log line or an error: it is
//! wrapped in `SecretString` and resolved at runtime from the environment,
//! the keychain or SSM (§57, house rule on secrets).

pub mod breaker;
pub mod jev;
pub mod keys;
pub mod ratelimit;
pub mod scripted;
pub mod secret;

pub use breaker::{BreakerState, CircuitBreaker};
pub use jev::{JevConfig, JevJudge, UsageRecorder};
pub use keys::JevKeySource;
pub use ratelimit::TokenBucket;
pub use scripted::{NullJudge, ScriptedJudge};
pub use secret::SecretString;
