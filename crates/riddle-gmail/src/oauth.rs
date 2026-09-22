//! OAuth 2.0 for a desktop app: authorization code with PKCE and a loopback
//! redirect (RFC 8252, §16 "Desktop apps cannot keep a client secret"). The
//! listener binds `127.0.0.1` on a random port before the browser opens, so
//! a consent that completes instantly still has somewhere to land. `state`
//! guards the unauthenticated local endpoint against a planted code.
//!
//! Google's "Desktop app" client type issues a client secret that is not a
//! secret; it is sent when configured because the token endpoint requires
//! it for that type. The refresh token goes to the keychain through
//! `riddle-credentials`; access tokens live in memory and refresh a minute
//! before expiry.

use crate::parse::map_http_error;
use crate::types::TokenResponse;
use base64::Engine;
use parking_lot::Mutex;
use riddle_core::DigId;
use riddle_credentials::{gmail_refresh_token_key, Credentials, GMAIL_CLIENT_SECRET};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const GMAIL_MODIFY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";
pub const DEFAULT_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// How long the user gets to finish consent.
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct GmailOAuthConfig {
    pub client_id: String,
    /// The "Desktop app" client secret, when Google issued one. Falls back to
    /// the `gmail_client_secret` keychain item.
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub auth_url: String,
    pub token_url: String,
}

impl GmailOAuthConfig {
    pub fn new(client_id: impl Into<String>, client_secret: Option<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret,
            scopes: vec![GMAIL_MODIFY_SCOPE.into()],
            auth_url: DEFAULT_AUTH_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
        }
    }
    fn secret(&self, credentials: &Credentials) -> Option<String> {
        self.client_secret
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                credentials
                    .get(GMAIL_CLIENT_SECRET)
                    .ok()
                    .flatten()
                    .filter(|s| !s.is_empty())
            })
    }
}

fn random_token(bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// RFC 7636 S256: `BASE64URL(SHA256(verifier))`.
pub fn code_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn build_auth_url(
    config: &GmailOAuthConfig,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    login_hint: Option<&str>,
) -> Result<String> {
    let scope = config.scopes.join(" ");
    let mut params = vec![
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", scope.as_str()),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("include_granted_scopes", "true"),
    ];
    if let Some(h) = login_hint {
        params.push(("login_hint", h));
    }
    url::Url::parse_with_params(&config.auth_url, &params)
        .map(|u| u.to_string())
        .map_err(|e| RiddleError::internal(format!("auth url: {e}")))
}

/// Parse `GET /callback?code=…&state=… HTTP/1.1` into its query pairs.
pub fn parse_callback_request(request: &str) -> HashMap<String, String> {
    let Some(line) = request.lines().next() else {
        return HashMap::new();
    };
    let Some(target) = line.split_whitespace().nth(1) else {
        return HashMap::new();
    };
    url::Url::parse("http://callback.invalid")
        .and_then(|b| b.join(target))
        .map(|u| u.query_pairs().into_owned().collect())
        .unwrap_or_default()
}

fn page(message: &str) -> String {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Riddle</title><body style=\"font:15px -apple-system,system-ui,sans-serif;padding:3rem;color:#222\"><p>{message}</p><p style=\"color:#777\">You can close this tab and return to Riddle.</p>"
    );
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// A sign-in in progress: the listener is bound, the URL is ready to open.
/// Debug shows the port only; the verifier and state stay private.
pub struct AuthFlow {
    config: GmailOAuthConfig,
    listener: TcpListener,
    redirect_uri: String,
    state: String,
    verifier: String,
    auth_url: String,
}

impl std::fmt::Debug for AuthFlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthFlow")
            .field("redirect_uri", &self.redirect_uri)
            .finish()
    }
}

impl AuthFlow {
    /// Bind the loopback listener and build the consent URL.
    pub async fn begin(config: GmailOAuthConfig, login_hint: Option<&str>) -> Result<Self> {
        if config.client_id.trim().is_empty() {
            return Err(RiddleError::invalid(
                "a Gmail OAuth client id is required; add it in settings",
            ));
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| {
            RiddleError::internal(format!(
                "cannot listen on loopback for the sign-in redirect: {e}"
            ))
        })?;
        let port = listener.local_addr().map_err(RiddleError::internal)?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let state = random_token(24);
        let verifier = random_token(48);
        let auth_url = build_auth_url(
            &config,
            &redirect_uri,
            &state,
            &code_challenge(&verifier),
            login_hint,
        )?;
        rinfo!(
            Category::Credentials,
            "gmail sign-in started on loopback port {port}"
        );
        Ok(Self {
            config,
            listener,
            redirect_uri,
            state,
            verifier,
            auth_url,
        })
    }

    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Wait for the browser to come back with a code.
    pub async fn wait_for_code(&self, timeout: Duration) -> Result<String> {
        let accept = async {
            loop {
                let (mut socket, _) = self.listener.accept().await?;
                let mut buf = vec![0u8; 8192];
                let n = socket.read(&mut buf).await?;
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let line = request.lines().next().unwrap_or_default();
                if !line.contains("/callback") {
                    let _ = socket
                        .write_all(page("Waiting for Google…").as_bytes())
                        .await;
                    continue;
                }
                let params = parse_callback_request(&request);
                if let Some(err) = params.get("error") {
                    let _ = socket
                        .write_all(page("Google declined the sign-in.").as_bytes())
                        .await;
                    let kind_msg = if err == "admin_policy_enforced"
                        || err == "access_denied"
                            && params
                                .get("error_description")
                                .is_some_and(|d| d.contains("admin"))
                    {
                        return Ok::<_, std::io::Error>(Err(RiddleError::new(
                            riddle_errors::ErrorKind::ProviderAdminBlocked,
                            crate::parse::admin_blocked_message(&self.config.client_id),
                        )));
                    } else {
                        format!("Google declined the sign-in: {err}")
                    };
                    return Ok(Err(RiddleError::provider_auth(kind_msg)));
                }
                if params.get("state").map(String::as_str) != Some(self.state.as_str()) {
                    let _ = socket
                        .write_all(
                            page("That sign-in did not match the request that started it.")
                                .as_bytes(),
                        )
                        .await;
                    return Ok(Err(RiddleError::provider_auth(
                        "the sign-in did not match the request that started it; try again",
                    )));
                }
                let Some(code) = params.get("code").cloned() else {
                    let _ = socket
                        .write_all(page("No authorization code came back.").as_bytes())
                        .await;
                    return Ok(Err(RiddleError::provider_auth(
                        "Google redirected back without an authorization code",
                    )));
                };
                let _ = socket
                    .write_all(page("Riddle is connected to Gmail.").as_bytes())
                    .await;
                let _ = socket.flush().await;
                return Ok(Ok(code));
            }
        };
        match tokio::time::timeout(timeout, accept).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => Err(RiddleError::internal(format!(
                "waiting for the Google redirect failed: {e}"
            ))),
            Err(_) => Err(RiddleError::provider_auth(
                "timed out waiting for the Google sign-in to finish",
            )),
        }
    }

    /// Exchange the code and store the refresh token for `dig`. Returns the
    /// token response (the access token is useful immediately).
    pub async fn finish(
        self,
        http: &reqwest::Client,
        credentials: &Credentials,
        dig: &DigId,
        code: &str,
    ) -> Result<TokenResponse> {
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("client_id", self.config.client_id.clone()),
            ("code", code.to_string()),
            ("code_verifier", self.verifier.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
        ];
        if let Some(secret) = self.config.secret(credentials) {
            form.push(("client_secret", secret));
        }
        let tokens = post_token(http, &self.config, &form).await?;
        match &tokens.refresh_token {
            Some(rt) => credentials.set(&gmail_refresh_token_key(dig), rt)?,
            None => return Err(RiddleError::provider_auth("Google did not return a refresh token; remove Riddle from the account's connected apps and sign in again")),
        }
        rinfo!(
            Category::Credentials,
            "gmail refresh token stored for dig {}",
            dig
        );
        Ok(tokens)
    }

    /// Run the whole flow after the caller opened `auth_url` in the browser.
    pub async fn complete(
        self,
        http: &reqwest::Client,
        credentials: &Credentials,
        dig: &DigId,
    ) -> Result<TokenResponse> {
        let code = self.wait_for_code(CONSENT_TIMEOUT).await?;
        self.finish(http, credentials, dig, &code).await
    }
}

async fn post_token(
    http: &reqwest::Client,
    config: &GmailOAuthConfig,
    form: &[(&str, String)],
) -> Result<TokenResponse> {
    let resp = http
        .post(&config.token_url)
        .form(form)
        .send()
        .await
        .map_err(|e| RiddleError::provider(format!("cannot reach Google's token endpoint: {e}")))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(map_http_error(
            status,
            None,
            &text,
            false,
            &config.client_id,
        ));
    }
    serde_json::from_str(&text)
        .map_err(|e| RiddleError::provider(format!("Google returned a token we cannot read: {e}")))
}

struct Access {
    token: String,
    expires_at: Instant,
}

/// Access tokens for one Dig, refreshed from the keychain's refresh token.
pub struct TokenManager {
    config: GmailOAuthConfig,
    credentials: Arc<Credentials>,
    dig: DigId,
    http: reqwest::Client,
    access: Mutex<Option<Access>>,
}

impl TokenManager {
    pub fn new(
        config: GmailOAuthConfig,
        credentials: Arc<Credentials>,
        dig: DigId,
        http: reqwest::Client,
    ) -> Self {
        Self {
            config,
            credentials,
            dig,
            http,
            access: Mutex::new(None),
        }
    }

    pub fn client_id(&self) -> &str {
        &self.config.client_id
    }

    pub fn signed_in(&self) -> bool {
        self.credentials
            .present(&gmail_refresh_token_key(&self.dig))
    }

    /// Drop the cached access token (after a 401).
    pub fn invalidate(&self) {
        *self.access.lock() = None;
    }

    /// A token valid for at least a minute.
    pub async fn access_token(&self) -> Result<String> {
        if let Some(a) = self.access.lock().as_ref() {
            if a.expires_at > Instant::now() + Duration::from_secs(60) {
                return Ok(a.token.clone());
            }
        }
        let refresh = self
            .credentials
            .get(&gmail_refresh_token_key(&self.dig))?
            .ok_or_else(|| RiddleError::provider_auth("not signed in to Gmail for this Dig"))?;
        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("client_id", self.config.client_id.clone()),
            ("refresh_token", refresh),
        ];
        if let Some(secret) = self.config.secret(&self.credentials) {
            form.push(("client_secret", secret));
        }
        let tokens = match post_token(&self.http, &self.config, &form).await {
            Ok(t) => t,
            Err(e) => {
                rwarn!(
                    Category::Credentials,
                    "gmail token refresh for dig {} failed: {}",
                    self.dig,
                    e.kind.as_str()
                );
                return Err(e);
            }
        };
        if let Some(rt) = &tokens.refresh_token {
            // Google rotates refresh tokens rarely, but honour it when it does.
            self.credentials
                .set(&gmail_refresh_token_key(&self.dig), rt)?;
        }
        let ttl = Duration::from_secs(tokens.expires_in.unwrap_or(3600));
        *self.access.lock() = Some(Access {
            token: tokens.access_token.clone(),
            expires_at: Instant::now() + ttl,
        });
        Ok(tokens.access_token)
    }

    /// Forget the Dig's refresh token.
    pub fn sign_out(&self) -> Result<()> {
        self.invalidate();
        self.credentials.delete(&gmail_refresh_token_key(&self.dig))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pkce_challenge_matches_the_rfc_7636_vector() {
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn the_auth_url_asks_for_offline_access_with_pkce_and_state() {
        let cfg = GmailOAuthConfig::new("cid.apps.googleusercontent.com", None);
        let u = build_auth_url(
            &cfg,
            "http://127.0.0.1:4242/callback",
            "st",
            "ch",
            Some("matt@example.com"),
        )
        .unwrap();
        let parsed = url::Url::parse(&u).unwrap();
        let q: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["code_challenge"], "ch");
        assert_eq!(q["state"], "st");
        assert_eq!(q["scope"], GMAIL_MODIFY_SCOPE);
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:4242/callback");
        assert_eq!(q["login_hint"], "matt@example.com");
        assert_eq!(q["prompt"], "consent");
    }

    #[test]
    fn the_callback_request_line_yields_code_and_state() {
        let p = parse_callback_request("GET /callback?state=st&code=4%2F0Abc-def&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fgmail.modify HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        assert_eq!(p["code"], "4/0Abc-def");
        assert_eq!(p["state"], "st");
        assert!(parse_callback_request("").is_empty());
    }

    #[tokio::test]
    async fn the_loopback_listener_accepts_the_redirect_and_rejects_a_wrong_state() {
        let cfg = GmailOAuthConfig::new("cid", None);
        let flow = AuthFlow::begin(cfg.clone(), None).await.unwrap();
        let uri = flow.redirect_uri().to_string();
        let port: u16 = uri
            .rsplit(':')
            .next()
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let state = flow.state.clone();
        tokio::spawn(async move {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            s.write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            s.write_all(
                format!("GET /callback?state={state}&code=abc123 HTTP/1.1\r\nHost: x\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
            let mut out = Vec::new();
            let _ = s.read_to_end(&mut out).await;
            assert!(String::from_utf8_lossy(&out).contains("connected"));
        });
        let code = flow.wait_for_code(Duration::from_secs(5)).await.unwrap();
        assert_eq!(code, "abc123");

        let flow = AuthFlow::begin(cfg, None).await.unwrap();
        let port: u16 = flow
            .redirect_uri()
            .rsplit(':')
            .next()
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        tokio::spawn(async move {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            s.write_all(b"GET /callback?state=wrong&code=abc HTTP/1.1\r\nHost: x\r\n\r\n")
                .await
                .unwrap();
            let mut out = Vec::new();
            let _ = s.read_to_end(&mut out).await;
        });
        let err = flow
            .wait_for_code(Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::ProviderAuth);
    }

    #[test]
    fn an_empty_client_id_is_refused_before_anything_binds() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(AuthFlow::begin(GmailOAuthConfig::new("", None), None))
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
    }

    #[tokio::test]
    async fn the_token_manager_refuses_without_a_refresh_token_and_never_touches_the_keychain() {
        let creds = Arc::new(Credentials::in_memory());
        let tm = TokenManager::new(
            GmailOAuthConfig::new("cid", None),
            creds.clone(),
            DigId::from("d1"),
            reqwest::Client::new(),
        );
        assert!(!tm.signed_in());
        let err = tm.access_token().await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::ProviderAuth);
        creds
            .set(&gmail_refresh_token_key(&DigId::from("d1")), "rt")
            .unwrap();
        assert!(tm.signed_in());
        tm.sign_out().unwrap();
        assert!(!tm.signed_in());
    }
}
