//! The OS keychain, behind a service name that differs between dev and prod
//! builds (`ARCHITECTURE.md` §58, §59).
//!
//! Two decisions. First, the backend is a trait so tests use an in-memory
//! store and never touch the real keychain: a dismissed macOS access dialog is
//! remembered as a refusal, and one refused prompt during a test run once
//! turned into "not connected" across a whole app. Second, the wrapper never
//! logs or returns a value in an error message; callers get `present(key)` for
//! the webview and the value only when they are about to use it.
//!
//! Keys in use: `gmail_refresh_token:<dig>`, `gmail_client_secret`,
//! `jev_api_key`.

use parking_lot::Mutex;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rdebug, rwarn, Category};
use std::collections::HashMap;
use std::sync::Arc;

/// Key for the refresh token of one Dig.
pub fn gmail_refresh_token_key(dig: &riddle_core::DigId) -> String {
    format!("gmail_refresh_token:{}", dig.as_str())
}
pub const GMAIL_CLIENT_SECRET: &str = "gmail_client_secret";
pub const JEV_API_KEY: &str = "jev_api_key";

/// Where secrets live. Implementations must never log values.
pub trait CredentialBackend: Send + Sync {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>>;
    fn set(&self, service: &str, key: &str, value: &str) -> Result<()>;
    fn delete(&self, service: &str, key: &str) -> Result<()>;
}

/// The real OS keychain through `keyring` with native features enabled.
pub struct KeyringBackend;

impl KeyringBackend {
    fn entry(service: &str, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(service, key)
            .map_err(|e| RiddleError::internal(format!("cannot reach the OS keychain: {e}")))
    }

    /// A refusal, explained in terms of what to do about it. The platform's
    /// own text for a denied access dialog sounds like a wrong password.
    fn refused(service: &str, key: &str, action: &str, e: keyring::Error) -> RiddleError {
        let advice = match &e {
            keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_) => format!(
                " The OS asks permission the first time a new build touches this item and remembers a dismissed dialog as a refusal. \
                 Open Keychain Access, find the \"{service}\" item \"{key}\" and allow Riddle under Access Control, or remove it and sign in again."
            ),
            _ => String::new(),
        };
        RiddleError::internal(format!("cannot {action} the OS keychain: {e}.{advice}"))
    }
}

impl CredentialBackend for KeyringBackend {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>> {
        match Self::entry(service, key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Self::refused(service, key, "read", e)),
        }
    }
    fn set(&self, service: &str, key: &str, value: &str) -> Result<()> {
        Self::entry(service, key)?
            .set_password(value)
            .map_err(|e| Self::refused(service, key, "write to", e))
    }
    fn delete(&self, service: &str, key: &str) -> Result<()> {
        match Self::entry(service, key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Self::refused(service, key, "clear", e)),
        }
    }
}

/// An in-memory backend for tests and the test-host guard.
#[derive(Default)]
pub struct MemoryBackend {
    items: Mutex<HashMap<(String, String), String>>,
}

impl CredentialBackend for MemoryBackend {
    fn get(&self, service: &str, key: &str) -> Result<Option<String>> {
        Ok(self
            .items
            .lock()
            .get(&(service.to_string(), key.to_string()))
            .cloned())
    }
    fn set(&self, service: &str, key: &str, value: &str) -> Result<()> {
        self.items
            .lock()
            .insert((service.to_string(), key.to_string()), value.to_string());
        Ok(())
    }
    fn delete(&self, service: &str, key: &str) -> Result<()> {
        self.items
            .lock()
            .remove(&(service.to_string(), key.to_string()));
        Ok(())
    }
}

/// The credential store the app registers as a service. `service` is the
/// keychain service name, e.g. `riddle` or `riddle-dev`.
pub struct Credentials {
    service: String,
    backend: Arc<dyn CredentialBackend>,
    /// Values read once per process so a keychain prompt happens per launch,
    /// not per poll. Failures are not cached so "Always Allow" takes effect.
    cache: Mutex<HashMap<String, Option<String>>>,
}

impl Credentials {
    pub fn new(service: impl Into<String>, backend: Arc<dyn CredentialBackend>) -> Self {
        Self {
            service: service.into(),
            backend,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The real keychain under `riddle` or `riddle-dev` (§59).
    pub fn keyring(dev: bool) -> Self {
        let service = if dev { "riddle-dev" } else { "riddle" };
        Self::new(service, Arc::new(KeyringBackend))
    }

    /// Never touches the keychain.
    pub fn in_memory() -> Self {
        Self::new("riddle-test", Arc::new(MemoryBackend::default()))
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        if let Some(hit) = self.cache.lock().get(key) {
            return Ok(hit.clone());
        }
        let value = self.backend.get(&self.service, key)?;
        self.cache.lock().insert(key.to_string(), value.clone());
        rdebug!(
            Category::Credentials,
            "read {} ({})",
            redact_key(key),
            if value.is_some() { "present" } else { "absent" }
        );
        Ok(value)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.backend.set(&self.service, key, value)?;
        self.cache
            .lock()
            .insert(key.to_string(), Some(value.to_string()));
        rdebug!(Category::Credentials, "stored {}", redact_key(key));
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        self.backend.delete(&self.service, key)?;
        self.cache.lock().insert(key.to_string(), None);
        rdebug!(Category::Credentials, "deleted {}", redact_key(key));
        Ok(())
    }

    /// The only thing the webview is told about a secret.
    pub fn present(&self, key: &str) -> bool {
        match self.get(key) {
            Ok(v) => v.is_some_and(|s| !s.is_empty()),
            Err(e) => {
                rwarn!(
                    Category::Credentials,
                    "presence check for {} failed: {}",
                    redact_key(key),
                    e.kind.as_str()
                );
                false
            }
        }
    }

    /// Drop the process cache, e.g. after the user changed the keychain by hand.
    pub fn forget_cache(&self) {
        self.cache.lock().clear();
    }
}

/// Keys carry a Dig id after the colon; that is an identifier and safe to log.
fn redact_key(key: &str) -> String {
    key.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_round_trips_and_presence_reflects_it() {
        let c = Credentials::in_memory();
        assert!(!c.present(JEV_API_KEY));
        c.set(JEV_API_KEY, "k-1").unwrap();
        assert!(c.present(JEV_API_KEY));
        assert_eq!(c.get(JEV_API_KEY).unwrap().as_deref(), Some("k-1"));
        c.delete(JEV_API_KEY).unwrap();
        assert!(!c.present(JEV_API_KEY));
        assert_eq!(c.get(JEV_API_KEY).unwrap(), None);
    }

    #[test]
    fn the_refresh_token_key_is_scoped_to_the_dig() {
        let dig = riddle_core::DigId::from("dig-1");
        assert_eq!(gmail_refresh_token_key(&dig), "gmail_refresh_token:dig-1");
    }

    #[test]
    fn an_empty_value_does_not_count_as_present() {
        let c = Credentials::in_memory();
        c.set(GMAIL_CLIENT_SECRET, "").unwrap();
        assert!(!c.present(GMAIL_CLIENT_SECRET));
    }
}
