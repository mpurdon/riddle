//! Where the Jev key comes from (§57): the environment for scripts, the OS
//! keychain for daily use, SSM Parameter Store for the owner's dev
//! profile. Resolution happens at runtime and the value is never written to
//! disk in this repository. An SSM failure says which parameter and profile
//! were tried, never what was found.

use crate::secret::SecretString;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JevKeySource {
    /// Environment variables tried in order; the first non-empty wins.
    Env { vars: Vec<String> },
    /// The OS keychain entry `service`/`account`.
    Keychain { service: String, account: String },
    /// SSM Parameter Store via a named AWS profile.
    Ssm {
        profile: String,
        parameter: String,
        region: String,
    },
}

impl JevKeySource {
    pub fn env() -> Self {
        Self::Env {
            vars: vec!["TYPESAFE_API_KEY".into(), "RIDDLE_JEV_API_KEY".into()],
        }
    }
    pub fn keychain(service: impl Into<String>) -> Self {
        Self::Keychain {
            service: service.into(),
            account: "jev".into(),
        }
    }
    pub fn ssm(profile: impl Into<String>, parameter: impl Into<String>) -> Self {
        Self::Ssm {
            profile: profile.into(),
            parameter: parameter.into(),
            region: "us-east-1".into(),
        }
    }
    /// The owner's setup for live tests, from `RIDDLE_DEV_AWS_PROFILE` and
    /// `RIDDLE_DEV_JEV_SSM_PARAMETER` (a git-ignored `.env` at the repository
    /// root; see `.env.example`). `None` until both are set.
    pub fn dev_ssm() -> Option<Self> {
        let read = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        Some(Self::ssm(
            read("RIDDLE_DEV_AWS_PROFILE")?,
            read("RIDDLE_DEV_JEV_SSM_PARAMETER")?,
        ))
    }

    pub fn describe(&self) -> String {
        match self {
            JevKeySource::Env { vars } => format!("env:{}", vars.join("|")),
            JevKeySource::Keychain { service, .. } => format!("keychain:{service}"),
            JevKeySource::Ssm {
                profile, parameter, ..
            } => format!("ssm:{profile}:{parameter}"),
        }
    }

    pub async fn resolve(&self) -> Result<SecretString> {
        let key = match self {
            JevKeySource::Env { vars } => vars
                .iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.trim().is_empty()))
                .map(SecretString::new)
                .ok_or_else(|| {
                    RiddleError::judge_unavailable(format!("no Jev key in {}", vars.join(" or ")))
                })?,
            JevKeySource::Keychain { service, account } => {
                let entry = keyring::Entry::new(service, account).map_err(|e| {
                    RiddleError::judge_unavailable(format!("keychain entry {service}: {e}"))
                })?;
                match entry.get_password() {
                    Ok(p) => SecretString::new(p),
                    Err(keyring::Error::NoEntry) => {
                        return Err(RiddleError::judge_unavailable(format!(
                            "no Jev key in keychain service {service}"
                        )))
                    }
                    Err(e) => {
                        return Err(RiddleError::judge_unavailable(format!(
                            "keychain read failed for {service}: {e}"
                        )))
                    }
                }
            }
            JevKeySource::Ssm {
                profile,
                parameter,
                region,
            } => resolve_ssm(profile, parameter, region).await?,
        };
        if key.is_empty() {
            return Err(RiddleError::judge_unavailable(format!(
                "Jev key from {} is empty",
                self.describe()
            )));
        }
        rinfo!(
            Category::Credentials,
            "resolved Jev key from {}",
            self.describe()
        );
        Ok(key)
    }
}

async fn resolve_ssm(profile: &str, parameter: &str, region: &str) -> Result<SecretString> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .profile_name(profile)
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;
    let client = aws_sdk_ssm::Client::new(&config);
    let out = client.get_parameter().name(parameter).with_decryption(true).send().await.map_err(|e| {
        // The SDK error names the operation and the failure class; it does
        // not carry the parameter value.
        let kind = match &e {
            aws_sdk_ssm::error::SdkError::ServiceError(se) => format!("service error: {}", se.err().meta().code().unwrap_or("unknown")),
            aws_sdk_ssm::error::SdkError::DispatchFailure(_) => "dispatch failure (network or credentials)".to_string(),
            aws_sdk_ssm::error::SdkError::TimeoutError(_) => "timeout".to_string(),
            other => other.to_string().chars().take(120).collect::<String>(),
        };
        rwarn!(Category::Credentials, "SSM get_parameter failed for profile {profile}: {kind}");
        RiddleError::judge_unavailable(format!("could not read {parameter} via profile {profile}: {kind}; run `aws sso login --profile {profile}` if the session expired"))
    })?;
    let value = out
        .parameter()
        .and_then(|p| p.value())
        .map(|s| s.to_string())
        .ok_or_else(|| RiddleError::judge_unavailable(format!("{parameter} has no value")))?;
    Ok(SecretString::new(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_environment_source_reads_the_first_non_empty_variable() {
        let var = "RIDDLE_TEST_JEV_KEY_A";
        let var_b = "RIDDLE_TEST_JEV_KEY_B";
        std::env::remove_var(var);
        std::env::set_var(var_b, "k-b");
        let src = JevKeySource::Env {
            vars: vec![var.into(), var_b.into()],
        };
        assert_eq!(src.resolve().await.unwrap().expose_secret(), "k-b");
        std::env::remove_var(var_b);
        let err = src.resolve().await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::JudgeUnavailable);
        assert!(!err.message.contains("k-b"));
    }
}
