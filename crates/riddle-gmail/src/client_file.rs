//! The OAuth client JSON Google Auth Platform offers for download
//! (`client_secret_<id>.json`), read so the user can import it instead of
//! copying two strings by hand.
//!
//! A Desktop client's file has the shape `{"installed": {"client_id",
//! "client_secret", ...}}`. A Web client's file uses `"web"` instead, and
//! Riddle refuses it: its loopback redirect only works with a Desktop client.
//! Errors never quote the file, because it contains the secret.

use riddle_errors::{Result, RiddleError};
use serde::Deserialize;

/// What Riddle keeps from the file: the id goes to settings, the secret to
/// the keychain.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientFile {
    pub client_id: String,
    pub client_secret: Option<String>,
    /// The Google Cloud project, shown so the user can tell which file they chose.
    pub project_id: Option<String>,
}

impl std::fmt::Debug for ClientFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientFile")
            .field("client_id", &self.client_id)
            .field("project_id", &self.project_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[derive(Deserialize)]
struct Raw {
    installed: Option<Client>,
    web: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Client {
    client_id: Option<String>,
    client_secret: Option<String>,
    project_id: Option<String>,
}

pub fn parse_client_file(text: &str) -> Result<ClientFile> {
    let raw: Raw = serde_json::from_str(text).map_err(|_| {
        RiddleError::invalid(
            "that file is not the OAuth client JSON Google Auth Platform downloads",
        )
    })?;
    let Some(client) = raw.installed else {
        return Err(RiddleError::invalid(if raw.web.is_some() {
            "that file is for a Web application client; Riddle needs a Desktop app client (Google Auth Platform → Clients → Create client → Desktop app)"
        } else {
            "that file has no Desktop app client in it"
        }));
    };
    let client_id = client
        .client_id
        .map(|s| s.trim().to_string())
        .filter(|s| s.ends_with(".apps.googleusercontent.com"))
        .ok_or_else(|| RiddleError::invalid("the file has no Google client id"))?;
    let client_secret = client
        .client_secret
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(ClientFile {
        client_id,
        client_secret,
        project_id: client.project_id.filter(|s| !s.trim().is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: &str = r#"{"installed":{"client_id":"123-abc.apps.googleusercontent.com","project_id":"riddle-509505","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","client_secret":"GOCSPX-test","redirect_uris":["http://localhost"]}}"#;

    #[test]
    fn a_desktop_client_file_yields_the_id_and_the_secret() {
        let f = parse_client_file(DESKTOP).unwrap();
        assert_eq!(f.client_id, "123-abc.apps.googleusercontent.com");
        assert_eq!(f.client_secret.as_deref(), Some("GOCSPX-test"));
        assert_eq!(f.project_id.as_deref(), Some("riddle-509505"));
    }

    #[test]
    fn a_desktop_client_without_a_secret_is_accepted() {
        let f = parse_client_file(
            r#"{"installed":{"client_id":"123-abc.apps.googleusercontent.com"}}"#,
        )
        .unwrap();
        assert_eq!(f.client_secret, None);
    }

    #[test]
    fn a_web_client_file_is_refused_with_what_to_create_instead() {
        let err = parse_client_file(
            r#"{"web":{"client_id":"1-x.apps.googleusercontent.com","client_secret":"GOCSPX-web"}}"#,
        )
        .unwrap_err();
        assert!(err.message.contains("Desktop app"));
        assert!(!err.message.contains("GOCSPX"));
    }

    #[test]
    fn errors_and_debug_output_never_carry_the_secret() {
        let err = parse_client_file("{\"installed\": GOCSPX-broken").unwrap_err();
        assert!(!err.message.contains("GOCSPX"));
        let f = parse_client_file(DESKTOP).unwrap();
        assert!(!format!("{f:?}").contains("GOCSPX"));
    }
}
