//! The one error type every Riddle Action, service and command returns.
//!
//! `ARCHITECTURE.md` §8 fixes the taxonomy so the UI and the Director can
//! respond correctly: `provider_auth` means "offer sign in", `judge_unavailable`
//! means "degrade to Unresolved, hide nothing", `needs_confirmation` means
//! "return a pending action". A string error would make every caller sniff
//! text, which is how Pontifex lost a week to "your SSO token expired" surfacing
//! as an opaque dispatch failure. The kind is the contract; the message is for
//! humans; `detail` carries structured data the caller can act on.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The error kinds. Serialized in snake_case so the webview switches on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorKind {
    /// Sign in. The provider rejected or has no credential.
    ProviderAuth,
    /// A Workspace administrator blocks Riddle; explain what to ask for.
    ProviderAdminBlocked,
    /// Back off; park the job.
    ProviderRateLimited,
    /// The history cursor expired; a full resync is required.
    ProviderHistoryExpired,
    /// Any other provider failure (network, 5xx, malformed response).
    Provider,
    /// The semantic judge is unavailable; degrade to Unresolved, hide nothing.
    JudgeUnavailable,
    /// The LLM is unavailable; degrade to Unresolved, hide nothing.
    LlmUnavailable,
    /// A per-Dig budget is exhausted; pause the job, show it in the Task Centre.
    BudgetExhausted,
    /// Not permitted at this autonomy level or by this principal.
    PolicyDenied,
    /// The Action wants confirmation; the detail carries the pending action.
    NeedsConfirmation,
    /// Provider state changed underneath; reconcile before retrying.
    Conflict,
    /// The requested entity does not exist.
    NotFound,
    /// The caller's input failed validation before any side effect.
    Invalid,
    /// The local store refused a write or is faulted.
    Store,
    /// Anything else: IO, parsing, a bug.
    Internal,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::ProviderAuth => "provider_auth",
            ErrorKind::ProviderAdminBlocked => "provider_admin_blocked",
            ErrorKind::ProviderRateLimited => "provider_rate_limited",
            ErrorKind::ProviderHistoryExpired => "provider_history_expired",
            ErrorKind::Provider => "provider",
            ErrorKind::JudgeUnavailable => "judge_unavailable",
            ErrorKind::LlmUnavailable => "llm_unavailable",
            ErrorKind::BudgetExhausted => "budget_exhausted",
            ErrorKind::PolicyDenied => "policy_denied",
            ErrorKind::NeedsConfirmation => "needs_confirmation",
            ErrorKind::Conflict => "conflict",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Invalid => "invalid",
            ErrorKind::Store => "store",
            ErrorKind::Internal => "internal",
        }
    }

    /// Whether a retry after a pause is a reasonable response.
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            ErrorKind::ProviderRateLimited
                | ErrorKind::Provider
                | ErrorKind::JudgeUnavailable
                | ErrorKind::LlmUnavailable
        )
    }
}

/// `{kind, message, detail}`; see `ARCHITECTURE.md` §8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, thiserror::Error)]
#[error("{kind:?}: {message}")]
#[ts(export)]
pub struct RiddleError {
    pub kind: ErrorKind,
    pub message: String,
    /// Structured data for the caller: a pending action for
    /// `NeedsConfirmation`, a retry-after for `ProviderRateLimited`, the
    /// conflicting provider state for `Conflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown | null")]
    pub detail: Option<serde_json::Value>,
}

impl RiddleError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), detail: None }
    }
    pub fn with_detail(mut self, detail: impl Serialize) -> Self {
        self.detail = serde_json::to_value(detail).ok();
        self
    }
    pub fn provider_auth(m: impl Into<String>) -> Self { Self::new(ErrorKind::ProviderAuth, m) }
    pub fn provider(m: impl Into<String>) -> Self { Self::new(ErrorKind::Provider, m) }
    pub fn rate_limited(m: impl Into<String>) -> Self { Self::new(ErrorKind::ProviderRateLimited, m) }
    pub fn history_expired(m: impl Into<String>) -> Self { Self::new(ErrorKind::ProviderHistoryExpired, m) }
    pub fn judge_unavailable(m: impl Into<String>) -> Self { Self::new(ErrorKind::JudgeUnavailable, m) }
    pub fn llm_unavailable(m: impl Into<String>) -> Self { Self::new(ErrorKind::LlmUnavailable, m) }
    pub fn budget_exhausted(m: impl Into<String>) -> Self { Self::new(ErrorKind::BudgetExhausted, m) }
    pub fn policy_denied(m: impl Into<String>) -> Self { Self::new(ErrorKind::PolicyDenied, m) }
    pub fn needs_confirmation(m: impl Into<String>) -> Self { Self::new(ErrorKind::NeedsConfirmation, m) }
    pub fn conflict(m: impl Into<String>) -> Self { Self::new(ErrorKind::Conflict, m) }
    pub fn not_found(m: impl Into<String>) -> Self { Self::new(ErrorKind::NotFound, m) }
    pub fn invalid(m: impl Into<String>) -> Self { Self::new(ErrorKind::Invalid, m) }
    pub fn store(m: impl Into<String>) -> Self { Self::new(ErrorKind::Store, m) }
    pub fn internal(m: impl std::fmt::Display) -> Self { Self::new(ErrorKind::Internal, m.to_string()) }
}

pub type Result<T> = std::result::Result<T, RiddleError>;

impl From<std::io::Error> for RiddleError {
    fn from(e: std::io::Error) -> Self { RiddleError::internal(e) }
}
impl From<serde_json::Error> for RiddleError {
    fn from(e: serde_json::Error) -> Self { RiddleError::invalid(format!("invalid JSON: {e}")) }
}
impl From<std::fmt::Error> for RiddleError {
    fn from(e: std::fmt::Error) -> Self { RiddleError::internal(e) }
}

/// Convenience for `map_err(internal)`.
pub fn internal(e: impl std::fmt::Display) -> RiddleError { RiddleError::internal(e) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_serializes_to_kind_message_and_optional_detail() {
        let e = RiddleError::needs_confirmation("confirm send").with_detail(serde_json::json!({"pending": 1}));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "needs_confirmation");
        assert_eq!(v["message"], "confirm send");
        assert_eq!(v["detail"]["pending"], 1);
        let plain = serde_json::to_value(RiddleError::not_found("x")).unwrap();
        assert!(plain.get("detail").is_none());
    }

    #[test]
    fn transient_kinds_are_the_ones_worth_retrying() {
        assert!(ErrorKind::ProviderRateLimited.is_transient());
        assert!(!ErrorKind::PolicyDenied.is_transient());
    }
}
