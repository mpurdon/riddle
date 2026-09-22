//! `MailProvider` over the Gmail client, and the factory the registry uses
//! to build one per Dig from the OAuth config and the keychain.

use crate::client::{Format, GmailClient};
use crate::oauth::{GmailOAuthConfig, TokenManager};
use crate::parse::{changes_from_history, labels_from_json, message_from_json, refs_from_list};
use async_trait::async_trait;
use parking_lot::RwLock;
use riddle_core::{Dig, DigId, Message, ProviderKind, ProviderLabel};
use riddle_credentials::Credentials;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_mail::mime::{build_rfc822, new_message_id};
use riddle_mail::{
    Change, Detail, HistoryPage, IdPage, ListPage, MailProvider, MessageRef, OutgoingMessage,
    ProviderFactory, ProviderProfile, SentMessage,
};
use riddle_observability::{rwarn, Category};
use std::sync::Arc;

/// History pages followed in one `history()` call before handing back a
/// partial cursor; the next poll continues.
const MAX_HISTORY_PAGES: usize = 50;

pub struct GmailProvider {
    dig: DigId,
    client: GmailClient,
    labels: RwLock<Option<Vec<ProviderLabel>>>,
}

impl GmailProvider {
    pub fn new(dig: DigId, client: GmailClient) -> Self {
        Self {
            dig,
            client,
            labels: RwLock::new(None),
        }
    }
    pub fn client(&self) -> &GmailClient {
        &self.client
    }

    async fn labels_cached(&self) -> Result<Vec<ProviderLabel>> {
        if let Some(l) = self.labels.read().as_ref() {
            return Ok(l.clone());
        }
        let list = labels_from_json(&self.client.list_labels().await?);
        *self.labels.write() = Some(list.clone());
        Ok(list)
    }

    fn raw(&self, m: &OutgoingMessage) -> String {
        use base64::Engine;
        let id = new_message_id(&m.from.email);
        let rfc822 = build_rfc822(m, &id, chrono::Utc::now());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rfc822.as_bytes())
    }
}

#[async_trait]
impl MailProvider for GmailProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gmail
    }

    async fn profile(&self) -> Result<ProviderProfile> {
        let p = self.client.get_profile().await?;
        Ok(ProviderProfile {
            email: p.email_address.to_lowercase(),
            aliases: vec![],
            name: None,
            history_cursor: p.history_id,
            messages_total: p.messages_total,
        })
    }

    async fn list_labels(&self) -> Result<Vec<ProviderLabel>> {
        *self.labels.write() = None;
        self.labels_cached().await
    }

    async fn ensure_label(&self, name: &str) -> Result<ProviderLabel> {
        if let Some(l) = self
            .labels_cached()
            .await?
            .into_iter()
            .find(|l| l.name == name)
        {
            return Ok(l);
        }
        // Refresh once: another client may have created it since the cache.
        *self.labels.write() = None;
        if let Some(l) = self
            .labels_cached()
            .await?
            .into_iter()
            .find(|l| l.name == name)
        {
            return Ok(l);
        }
        let created = self.client.create_label(name).await?;
        let label = ProviderLabel {
            id: created.id,
            name: created.name,
            managed_by_riddle: riddle_mail::is_managed_label_name(name),
            system: false,
        };
        if let Some(list) = self.labels.write().as_mut() {
            list.push(label.clone());
        }
        Ok(label)
    }

    async fn list_message_ids(&self, page: &ListPage) -> Result<IdPage> {
        let q = GmailClient::list_query(page.after, page.before);
        let list = self
            .client
            .list_messages(
                q.as_deref(),
                page.label.as_deref(),
                page.page_token.as_deref(),
                page.max,
            )
            .await?;
        Ok(IdPage {
            refs: refs_from_list(&list),
            next_page_token: list.next_page_token,
            result_size_estimate: list.result_size_estimate,
        })
    }

    async fn get_message(&self, provider_id: &str, detail: Detail) -> Result<Message> {
        let format = match detail {
            Detail::Metadata => Format::Metadata,
            Detail::Full => Format::Full,
        };
        let m = self.client.get_message(provider_id, format).await?;
        Ok(message_from_json(&self.dig, &m))
    }

    async fn get_messages_batch(&self, ids: &[String], detail: Detail) -> Result<Vec<Message>> {
        let format = match detail {
            Detail::Metadata => Format::Metadata,
            Detail::Full => Format::Full,
        };
        Ok(self
            .client
            .get_messages(ids, format)
            .await?
            .iter()
            .map(|m| message_from_json(&self.dig, m))
            .collect())
    }

    async fn history(&self, cursor: &str) -> Result<HistoryPage> {
        let mut changes: Vec<Change> = Vec::new();
        let mut page_token: Option<String> = None;
        let mut latest: Option<String> = None;
        let mut last_record_id: Option<String> = None;
        for _ in 0..MAX_HISTORY_PAGES {
            let list = match self
                .client
                .history_list(cursor, page_token.as_deref(), 500)
                .await
            {
                Ok(l) => l,
                Err(e) if e.kind == ErrorKind::ProviderHistoryExpired => {
                    return Ok(HistoryPage {
                        changes: vec![],
                        next_cursor: None,
                        expired: true,
                    })
                }
                Err(e) => return Err(e),
            };
            changes.extend(changes_from_history(&list));
            if let Some(r) = list.history.last() {
                last_record_id = Some(r.id.clone());
            }
            latest = list.history_id.clone().or(latest);
            page_token = list.next_page_token.clone();
            if page_token.is_none() {
                return Ok(HistoryPage {
                    changes,
                    next_cursor: latest,
                    expired: false,
                });
            }
        }
        // Too much history for one pass: hand back what we have, cursor at
        // the last record so the next poll continues from there.
        rwarn!(
            Category::Sync,
            "dig {} history has more than {} pages; continuing next poll",
            self.dig,
            MAX_HISTORY_PAGES
        );
        Ok(HistoryPage {
            changes,
            next_cursor: last_record_id.or(latest),
            expired: false,
        })
    }

    async fn modify_labels(&self, ids: &[String], add: &[String], remove: &[String]) -> Result<()> {
        if ids.is_empty() || (add.is_empty() && remove.is_empty()) {
            return Ok(());
        }
        self.client.batch_modify(ids, add, remove).await
    }

    async fn create_draft(&self, message: &OutgoingMessage) -> Result<String> {
        let raw = self.raw(message);
        Ok(self
            .client
            .create_draft(&raw, message.thread_provider_id.as_deref())
            .await?
            .id)
    }

    async fn update_draft(
        &self,
        provider_draft_id: &str,
        message: &OutgoingMessage,
    ) -> Result<String> {
        let raw = self.raw(message);
        Ok(self
            .client
            .update_draft(
                provider_draft_id,
                &raw,
                message.thread_provider_id.as_deref(),
            )
            .await?
            .id)
    }

    async fn delete_draft(&self, provider_draft_id: &str) -> Result<()> {
        self.client.delete_draft(provider_draft_id).await
    }

    async fn send_draft(&self, provider_draft_id: &str) -> Result<SentMessage> {
        let m = self.client.send_draft(provider_draft_id).await?;
        Ok(SentMessage {
            thread_provider_id: m.thread_id.unwrap_or_else(|| m.id.clone()),
            provider_id: m.id,
        })
    }

    async fn send(&self, message: &OutgoingMessage) -> Result<SentMessage> {
        if message.recipients().is_empty() {
            return Err(RiddleError::invalid(
                "a message needs at least one recipient",
            ));
        }
        let raw = self.raw(message);
        let m = self
            .client
            .send_message(&raw, message.thread_provider_id.as_deref())
            .await?;
        Ok(SentMessage {
            thread_provider_id: m.thread_id.unwrap_or_else(|| m.id.clone()),
            provider_id: m.id,
        })
    }

    async fn trash(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            self.client.trash(id).await?;
        }
        Ok(())
    }

    async fn search(&self, query: &str, max: u32) -> Result<Vec<MessageRef>> {
        let list = self
            .client
            .list_messages(Some(query), None, None, max)
            .await?;
        Ok(refs_from_list(&list))
    }
}

/// Builds a `GmailProvider` per Dig; register with `ProviderRegistry::set_gmail_factory`.
pub struct GmailProviderFactory {
    config: RwLock<Option<GmailOAuthConfig>>,
    credentials: Arc<Credentials>,
    http: reqwest::Client,
}

impl GmailProviderFactory {
    pub fn new(config: Option<GmailOAuthConfig>, credentials: Arc<Credentials>) -> Self {
        Self {
            config: RwLock::new(config),
            credentials,
            http: crate::client::http_client(),
        }
    }
    /// The user supplies the client id in settings; update without a restart.
    pub fn set_config(&self, config: GmailOAuthConfig) {
        *self.config.write() = Some(config);
    }
    pub fn config(&self) -> Option<GmailOAuthConfig> {
        self.config.read().clone()
    }
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
    pub fn credentials(&self) -> &Arc<Credentials> {
        &self.credentials
    }
    pub fn token_manager(&self, dig: &DigId) -> Result<Arc<TokenManager>> {
        let config = self.config().ok_or_else(|| {
            RiddleError::provider_auth(
                "Gmail is not configured: add the OAuth client id in settings",
            )
        })?;
        Ok(Arc::new(TokenManager::new(
            config,
            self.credentials.clone(),
            dig.clone(),
            self.http.clone(),
        )))
    }
}

impl ProviderFactory for GmailProviderFactory {
    fn build(&self, dig: &Dig) -> Result<Arc<dyn MailProvider>> {
        let tokens = self.token_manager(&dig.id)?;
        if !tokens.signed_in() {
            return Err(RiddleError::provider_auth(format!(
                "Dig {} is not signed in to Gmail",
                dig.name
            )));
        }
        let client = GmailClient::new(self.http.clone(), tokens);
        Ok(Arc::new(GmailProvider::new(dig.id.clone(), client)))
    }
}
