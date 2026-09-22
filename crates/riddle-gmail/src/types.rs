//! The Gmail REST wire shapes we read and write. Only the fields Riddle uses;
//! serde ignores the rest.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub email_address: String,
    #[serde(default)]
    pub messages_total: Option<u64>,
    #[serde(default)]
    pub history_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelList {
    #[serde(default)]
    pub labels: Vec<Label>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLabel<'a> {
    pub name: &'a str,
    pub label_list_visibility: &'a str,
    pub message_list_visibility: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRefJson {
    pub id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageList {
    #[serde(default)]
    pub messages: Vec<MessageRefJson>,
    #[serde(default)]
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub result_size_estimate: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    #[serde(default)]
    pub attachment_id: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub data: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(default)]
    pub part_id: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub body: Body,
    #[serde(default)]
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub history_id: Option<String>,
    #[serde(default)]
    pub internal_date: Option<String>,
    #[serde(default)]
    pub size_estimate: u64,
    #[serde(default)]
    pub payload: Option<Part>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMessage {
    pub message: MessageRefJson,
    #[serde(default)]
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: String,
    #[serde(default)]
    pub messages_added: Vec<HistoryMessage>,
    #[serde(default)]
    pub messages_deleted: Vec<HistoryMessage>,
    #[serde(default)]
    pub labels_added: Vec<HistoryMessage>,
    #[serde(default)]
    pub labels_removed: Vec<HistoryMessage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryList {
    #[serde(default)]
    pub history: Vec<HistoryRecord>,
    #[serde(default)]
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub history_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchModify<'a> {
    pub ids: &'a [String],
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    pub add_label_ids: &'a [String],
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    pub remove_label_ids: &'a [String],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftBody {
    pub message: RawMessage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftResponse {
    pub id: String,
    #[serde(default)]
    pub message: Option<MessageRefJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorItem {
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    #[serde(default)]
    pub code: Option<u16>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub errors: Vec<ApiErrorItem>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Either `{ "error": { ... } }` (API) or `{ "error": "code", "error_description": ... }` (OAuth).
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEnvelope {
    #[serde(default)]
    pub error: Option<serde_json::Value>,
    #[serde(default)]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub scope: Option<String>,
}
