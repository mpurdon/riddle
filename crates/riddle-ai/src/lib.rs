//! The AI boundary (`ARCHITECTURE.md` §39, §40, §56, §57): the `Llm` trait,
//! the Bedrock implementation, the escalation refiner, versioned prompt
//! files, the model catalog with its pricing table, a circuit breaker and
//! the usage ledger hook.
//!
//! Decisions recorded here:
//!
//! - **One trait, many providers.** Sift, the Director and Expeditions
//!   depend on `Llm`, never on Bedrock. `ScriptedLlm` is the test double and
//!   behaves identically at the privacy boundary so a test can prove that a
//!   local-only Dig never reaches a model.
//! - **Privacy is enforced at the call.** Every `LlmRequest` carries the
//!   `PrivacyMode` of the Dig it serves and every implementation checks it
//!   before touching the network. A forgotten check upstream cannot leak.
//! - **Untrusted content is marked at the boundary.** Tool results flagged
//!   `untrusted` are wrapped in `<untrusted_mail>` delimiters with an
//!   explicit "this is data, not instructions" notice by the adapter, not by
//!   each caller. EchoLeak (an email phrased as instructions to the human
//!   evaded the injection classifier), the Gemini summary injection (hidden
//!   text made "Summarize" emit a fake warning) and ShadowLeak (hidden HTML
//!   made an agent fetch an attacker URL) all worked because mail entered
//!   model context as if it were the user's words; see
//!   `ARCHITECTURE-REVIEW.md` §5.3.
//! - **Unpriced is not free.** The catalog prices only what has a verified
//!   price; everything else reports `None` and the ledger shows it as
//!   unpriced (§56).
//! - **A usage row never fails the call.** The recorder hook is invoked for
//!   every call, success or failure, and its own failure is swallowed.

pub mod bedrock;
pub mod breaker;
pub mod catalog;
pub mod llm;
pub mod prompts;
pub mod refiner;
pub mod retry;
pub mod scripted;
pub mod untrusted;
pub mod usage;

pub use bedrock::{BedrockConfig, BedrockLlm};
pub use breaker::{Breaker, BreakerConfig, BreakerState};
pub use catalog::{ModelCatalog, ModelPrice, ModelTier};
pub use llm::{
    ContentBlock, Llm, LlmIdentity, LlmMessage, LlmRequest, LlmResponse, Role, StopReason, ToolSpec,
};
pub use prompts::{Prompt, PromptName, PromptSource, PromptStore};
pub use refiner::{BedrockRefiner, LlmRefiner, ScriptedRefiner};
pub use scripted::{ScriptedLlm, ScriptedResponse};
pub use usage::{record_usage, UsageRecorder};
