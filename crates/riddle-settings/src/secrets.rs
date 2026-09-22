//! Secrets never live in settings files (§12, §58); the file only says
//! whether one exists. `SecretPresence` is the seam the credentials crate
//! implements so this crate does not depend on it. Implementations must be
//! cheap: presence is asked on every settings read, so an SSM-backed answer
//! has to be cached, never fetched inline.

use crate::model::{JudgeSettings, KeySource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Secret {
    /// The judge API key, wherever `keySource` says it lives.
    JudgeApiKey {
        source: KeySource,
        ssm_profile: Option<String>,
        ssm_parameter: Option<String>,
    },
    /// The Gmail OAuth client secret (Web client type only).
    GmailClientSecret,
}

impl Secret {
    pub fn judge(j: &JudgeSettings) -> Self {
        Secret::JudgeApiKey {
            source: j.key_source,
            ssm_profile: j.ssm_profile.clone(),
            ssm_parameter: j.ssm_parameter.clone(),
        }
    }
}

pub trait SecretPresence: Send + Sync {
    fn is_present(&self, secret: &Secret) -> bool;
}

/// Nothing is present: tests, the CLI before sign-in.
pub struct NoSecrets;
impl SecretPresence for NoSecrets {
    fn is_present(&self, _secret: &Secret) -> bool {
        false
    }
}

/// A fixed answer per secret, for tests and the fixture profile.
#[derive(Default)]
pub struct FixedPresence {
    pub judge_key: bool,
    pub gmail_secret: bool,
}
impl SecretPresence for FixedPresence {
    fn is_present(&self, secret: &Secret) -> bool {
        match secret {
            Secret::JudgeApiKey { .. } => self.judge_key,
            Secret::GmailClientSecret => self.gmail_secret,
        }
    }
}
