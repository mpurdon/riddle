//! Gmail: OAuth PKCE with a loopback redirect, the REST client with a quota
//! limiter and taxonomy error mapping, and the `MailProvider` over it
//! (`ARCHITECTURE.md` §16). Unit-tested against recorded JSON; nothing here
//! touches the network in tests. Live tests are `#[ignore]` and need the
//! user's OAuth client (deferred, see `docs/BUILD_PLAN.md`).

pub mod client;
pub mod oauth;
pub mod parse;
pub mod provider;
pub mod quota;
pub mod types;

pub use client::{http_client, Format, GmailClient};
pub use oauth::{AuthFlow, GmailOAuthConfig, TokenManager, GMAIL_MODIFY_SCOPE};
pub use provider::{GmailProvider, GmailProviderFactory};
pub use quota::QuotaLimiter;
