//! The Gmail REST client: one method per endpoint Riddle uses, each
//! spending quota first and mapping failures to the taxonomy. A 401 is
//! retried once after a token refresh; a second 401 is `ProviderAuth`.
//! Bodies and tokens never appear in logs; ids and counts do.

use crate::oauth::TokenManager;
use crate::parse::{self, map_http_error};
use crate::quota::{cost, QuotaLimiter};
use crate::types::*;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_observability::{rdebug, rinfo, Category};
use serde::de::DeserializeOwned;
use std::sync::Arc;

pub const DEFAULT_BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

/// Gmail caps `batchModify` at 1,000 ids.
pub const BATCH_MODIFY_MAX: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Metadata,
    Full,
}

pub const METADATA_HEADERS: &[&str] = &[
    "From",
    "To",
    "Cc",
    "Bcc",
    "Subject",
    "Date",
    "Message-ID",
    "In-Reply-To",
    "References",
    "List-Id",
    "List-Unsubscribe",
    "Precedence",
    "Auto-Submitted",
    "Reply-To",
    "Authentication-Results",
];

/// How often one request waits out `rateLimitExceeded` before failing.
const MAX_THROTTLE_RETRIES: u32 = 3;

pub struct GmailClient {
    http: reqwest::Client,
    tokens: Arc<TokenManager>,
    limiter: QuotaLimiter,
    base: String,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Riddle/0.1")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap_or_default()
}

impl GmailClient {
    pub fn new(http: reqwest::Client, tokens: Arc<TokenManager>) -> Self {
        Self {
            http,
            tokens,
            limiter: QuotaLimiter::default(),
            base: DEFAULT_BASE_URL.into(),
        }
    }
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }
    pub fn tokens(&self) -> &TokenManager {
        &self.tokens
    }

    async fn send(
        &self,
        build: impl Fn(&reqwest::Client, &str) -> reqwest::RequestBuilder,
        units: u32,
        history: bool,
    ) -> Result<String> {
        self.limiter.acquire(units).await;
        let mut refreshed = false;
        let mut throttled = 0;
        loop {
            let token = self.tokens.access_token().await?;
            let resp = build(&self.http, &token).send().await.map_err(|e| {
                if e.is_timeout() || e.is_connect() {
                    RiddleError::provider(format!("Gmail unreachable: {e}"))
                } else {
                    RiddleError::provider(format!("Gmail request failed: {e}"))
                }
            })?;
            let status = resp.status().as_u16();
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let text = resp.text().await.unwrap_or_default();
            if (200..300).contains(&status) {
                return Ok(text);
            }
            if status == 401 && !refreshed {
                refreshed = true;
                self.tokens.invalidate();
                rdebug!(
                    Category::Sync,
                    "gmail 401; refreshing the access token once"
                );
                continue;
            }
            let err = map_http_error(status, retry_after, &text, history, self.tokens.client_id());
            if err.kind == ErrorKind::ProviderAuth {
                self.tokens.invalidate();
            }
            // Gmail's per-second limit clears in seconds; one throttled fetch
            // used to fail a whole page of the first sync. Wait as asked, a
            // few times, before handing the error to the job's backoff.
            // A daily quota does not clear in seconds; hand that straight to the
            // job's backoff instead of sleeping through three retries first.
            let daily_quota = err
                .detail
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(|v| v.as_str())
                .is_some_and(|r| r == "dailyLimitExceeded" || r == "quotaExceeded");
            if err.kind == ErrorKind::ProviderRateLimited
                && !daily_quota
                && throttled < MAX_THROTTLE_RETRIES
            {
                throttled += 1;
                let secs = retry_after.unwrap_or(1 << throttled).min(30);
                rdebug!(
                    Category::Sync,
                    "gmail rate limited; retry {throttled} in {secs}s"
                );
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                self.limiter.acquire(units).await;
                continue;
            }
            return Err(err);
        }
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        units: u32,
        history: bool,
    ) -> Result<T> {
        let url = format!("{}/{}", self.base, path);
        let text = self
            .send(
                |c, token| c.get(&url).bearer_auth(token).query(query),
                units,
                history,
            )
            .await?;
        serde_json::from_str(&text).map_err(|e| {
            RiddleError::provider(format!("Gmail returned an unreadable {path} response: {e}"))
        })
    }

    async fn post_json<B: serde::Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        units: u32,
    ) -> Result<T> {
        let url = format!("{}/{}", self.base, path);
        let text = self
            .send(
                |c, token| c.post(&url).bearer_auth(token).json(body),
                units,
                false,
            )
            .await?;
        if text.trim().is_empty() {
            return serde_json::from_str("null")
                .map_err(|e| RiddleError::provider(format!("{path}: {e}")));
        }
        serde_json::from_str(&text).map_err(|e| {
            RiddleError::provider(format!("Gmail returned an unreadable {path} response: {e}"))
        })
    }

    pub async fn get_profile(&self) -> Result<Profile> {
        self.get_json("profile", &[], cost::PROFILE, false).await
    }

    pub async fn list_labels(&self) -> Result<LabelList> {
        self.get_json("labels", &[], cost::LABELS_LIST, false).await
    }

    pub async fn create_label(&self, name: &str) -> Result<Label> {
        let body = NewLabel {
            name,
            label_list_visibility: "labelShow",
            message_list_visibility: "show",
        };
        let l: Label = self.post_json("labels", &body, cost::LABELS_CREATE).await?;
        rinfo!(Category::Projection, "gmail label created: {}", l.id);
        Ok(l)
    }

    pub async fn list_messages(
        &self,
        q: Option<&str>,
        label_ids: Option<&str>,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<MessageList> {
        let mut query: Vec<(&str, String)> =
            vec![("maxResults", max_results.clamp(1, 500).to_string())];
        if let Some(q) = q {
            query.push(("q", q.to_string()));
        }
        if let Some(l) = label_ids {
            query.push(("labelIds", l.to_string()));
        }
        if let Some(t) = page_token {
            query.push(("pageToken", t.to_string()));
        }
        self.get_json("messages", &query, cost::MESSAGES_LIST, false)
            .await
    }

    pub async fn get_message(&self, id: &str, format: Format) -> Result<GmailMessage> {
        let mut query: Vec<(&str, String)> = Vec::new();
        match format {
            Format::Metadata => {
                query.push(("format", "metadata".into()));
                for h in METADATA_HEADERS.iter().chain(riddle_core::KEPT_HEADERS) {
                    query.push(("metadataHeaders", (*h).into()));
                }
            }
            Format::Full => query.push(("format", "full".into())),
        }
        self.get_json(&format!("messages/{id}"), &query, cost::MESSAGES_GET, false)
            .await
    }

    /// Concurrent gets with a small fan-out. Batch HTTP does not reduce units
    /// (§16), so this is as good and much simpler. Missing ids are skipped.
    pub async fn get_messages(&self, ids: &[String], format: Format) -> Result<Vec<GmailMessage>> {
        use futures::stream::{self, StreamExt};
        let results: Vec<Result<GmailMessage>> = stream::iter(ids.iter().cloned())
            .map(|id| async move { self.get_message(&id, format).await })
            .buffer_unordered(6)
            .collect()
            .await;
        let mut out = Vec::with_capacity(results.len());
        for r in results {
            match r {
                Ok(m) => out.push(m),
                Err(e) if e.kind == ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        // Keep the caller's order (newest first from the listing).
        let order: std::collections::HashMap<&String, usize> =
            ids.iter().enumerate().map(|(i, id)| (id, i)).collect();
        out.sort_by_key(|m| order.get(&m.id).copied().unwrap_or(usize::MAX));
        Ok(out)
    }

    pub async fn history_list(
        &self,
        start_history_id: &str,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<HistoryList> {
        let mut query: Vec<(&str, String)> = vec![
            ("startHistoryId", start_history_id.to_string()),
            ("maxResults", max_results.clamp(1, 500).to_string()),
        ];
        if let Some(t) = page_token {
            query.push(("pageToken", t.to_string()));
        }
        self.get_json("history", &query, cost::HISTORY_LIST, true)
            .await
    }

    pub async fn batch_modify(
        &self,
        ids: &[String],
        add: &[String],
        remove: &[String],
    ) -> Result<()> {
        for chunk in ids.chunks(BATCH_MODIFY_MAX) {
            let body = BatchModify {
                ids: chunk,
                add_label_ids: add,
                remove_label_ids: remove,
            };
            let _: Option<serde_json::Value> = self
                .post_json("messages/batchModify", &body, cost::BATCH_MODIFY)
                .await?;
            rinfo!(
                Category::Projection,
                "gmail batchModify applied to {} messages (+{} -{})",
                chunk.len(),
                add.len(),
                remove.len()
            );
        }
        Ok(())
    }

    pub async fn create_draft(&self, raw: &str, thread_id: Option<&str>) -> Result<DraftResponse> {
        let body = DraftBody {
            message: RawMessage {
                raw: raw.into(),
                thread_id: thread_id.map(String::from),
            },
        };
        self.post_json("drafts", &body, cost::DRAFT_CREATE).await
    }

    pub async fn update_draft(
        &self,
        id: &str,
        raw: &str,
        thread_id: Option<&str>,
    ) -> Result<DraftResponse> {
        let body = DraftBody {
            message: RawMessage {
                raw: raw.into(),
                thread_id: thread_id.map(String::from),
            },
        };
        let url = format!("{}/drafts/{id}", self.base);
        let text = self
            .send(
                |c, token| c.put(&url).bearer_auth(token).json(&body),
                cost::DRAFT_UPDATE,
                false,
            )
            .await?;
        serde_json::from_str(&text).map_err(|e| {
            RiddleError::provider(format!("Gmail returned an unreadable draft response: {e}"))
        })
    }

    pub async fn delete_draft(&self, id: &str) -> Result<()> {
        let url = format!("{}/drafts/{id}", self.base);
        match self
            .send(
                |c, token| c.delete(&url).bearer_auth(token),
                cost::DRAFT_DELETE,
                false,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if e.kind == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn send_draft(&self, id: &str) -> Result<MessageRefJson> {
        let body = serde_json::json!({ "id": id });
        let m: MessageRefJson = self.post_json("drafts/send", &body, cost::SEND).await?;
        rinfo!(
            Category::Actions,
            "gmail sent draft {} as message {}",
            id,
            m.id
        );
        Ok(m)
    }

    pub async fn send_message(&self, raw: &str, thread_id: Option<&str>) -> Result<MessageRefJson> {
        let body = RawMessage {
            raw: raw.into(),
            thread_id: thread_id.map(String::from),
        };
        let m: MessageRefJson = self.post_json("messages/send", &body, cost::SEND).await?;
        rinfo!(Category::Actions, "gmail sent message {}", m.id);
        Ok(m)
    }

    pub async fn trash(&self, id: &str) -> Result<()> {
        let _: Option<serde_json::Value> = self
            .post_json(
                &format!("messages/{id}/trash"),
                &serde_json::json!({}),
                cost::TRASH,
            )
            .await?;
        Ok(())
    }

    /// The Gmail `q` for a date window.
    pub fn list_query(
        after: Option<chrono::DateTime<chrono::Utc>>,
        before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(a) = after {
            parts.push(format!("after:{}", a.format("%Y/%m/%d")));
        }
        if let Some(b) = before {
            parts.push(format!("before:{}", b.format("%Y/%m/%d")));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

pub use parse::message_from_json;
