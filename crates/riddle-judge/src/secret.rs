//! A string that refuses to be printed. Every credential in this crate lives
//! in one of these so a stray `{:?}` in a log line cannot leak it.

use std::fmt;

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// The only way to read the value; call sites are greppable.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_the_value() {
        let s = SecretString::new("sk-super-secret");
        let printed = format!("{s:?}");
        assert!(!printed.contains("super"));
        assert_eq!(s.expose_secret(), "sk-super-secret");
    }
}
