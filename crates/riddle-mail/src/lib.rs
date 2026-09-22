//! Mail: the provider contract, normalization, the sanitizer, the fixture
//! mailbox, the provider registry and the mail Actions (`ARCHITECTURE.md`
//! §14–§16, §57–§58). Gmail itself lives in `riddle-gmail`; this crate is
//! everything a provider-independent caller needs.

pub mod actions;
pub mod fixture;
pub mod mime;
pub mod normalize;
pub mod provider;
pub mod registry;
pub mod sanitize;
pub mod triggers;

pub use fixture::{FixtureProvider, FixtureSpec};
pub use normalize::{normalize_message, normalize_text, NormalizedText};
pub use provider::*;
pub use registry::{ProviderFactory, ProviderRegistry};
pub use sanitize::{sanitize_html, sanitize_html_with, Sanitized};
pub use triggers::RefreshTriggers;
