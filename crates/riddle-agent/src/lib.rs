//! The Director (`ARCHITECTURE.md` §2, §9, §29, §40, §58): the user's agent
//! inside Riddle, a client of the Action layer that reaches the mailbox
//! through read services and changes nothing except through `Runtime::invoke`.
//!
//! The controls in this crate exist because of shipped exploits against
//! email-reading assistants (`ARCHITECTURE-REVIEW.md` §5.3):
//!
//! - **EchoLeak** (Microsoft 365 Copilot): an email phrased as instructions
//!   evaded the injection classifier and exfiltrated data through a
//!   reference-style markdown image. Here, mail enters model context only as
//!   a tool result marked untrusted and wrapped in delimiters (§58 control
//!   1), and the prompt forbids remote images and unlisted links.
//! - **Gemini summary injection**: hidden text turned "Summarize" into a
//!   phishing warning. Riddle's answer is provenance: the Director cannot
//!   state a disposition; it reports Riddle's decision through `explain`.
//! - **ShadowLeak** (ChatGPT Deep Research + Gmail) and **Comet**
//!   (Perplexity): hidden instructions made the agent use its own tools to
//!   leak data. Here, once a task has read untrusted content the principal
//!   carries `read_untrusted: true` and the Action layer approval-gates every
//!   egress Action (§58 control 3); the model cannot un-set the flag.
//! - **Microsoft AI red-team memory poisoning**: a benign-looking mail was
//!   stored as a rule and later forwarded 40% of project mail. Here, durable
//!   policy (Meshes, Signals, Never Hidden) is only ever a proposal the user
//!   approves with the Action layer's real arguments in front of them (§58
//!   controls 4 and 6), and conversations are not state (§43).
//!
//! Contamination-classified mail is refused to the Director unless the user
//! opened it deliberately (§58 control 2).

pub mod allowance;
pub mod corrections;
pub mod director;
pub mod fit;
pub mod reads;
pub mod tool_loop;
pub mod tools;

pub use corrections::{suggest_mesh_updates, Suggestion};
pub use director::{Director, DirectorConfig, SendRequest};
pub use reads::{DirectorReads, DirectorReadsHandle, ReadScope, ScriptedReads};
pub use tool_loop::{
    LoopBudget, LoopEvent, LoopOutcome, LoopPrincipal, LoopRequest, LoopStop, ToolLoop,
};
pub use tools::{redact_args, ToolKind};
