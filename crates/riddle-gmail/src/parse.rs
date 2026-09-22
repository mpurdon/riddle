//! Wire → domain: messages (with MIME payload walking), history records,
//! labels, and the error mapping to the §8 taxonomy. Pure functions, tested
//! against recorded JSON so the mapping is pinned without a network.

use crate::types::*;
use base64::Engine;
use riddle_core::{
    AttachmentMeta, DigId, Message, MessageHeaders, MessageId, ProviderLabel, ThreadId,
};
use riddle_errors::{ErrorKind, RiddleError};
use riddle_mail::mime::{
    decode_rfc2047, parse_address, parse_address_list, parse_authentication_results,
    parse_message_ids,
};
use riddle_mail::normalize::decode_entities;
use riddle_mail::{Change, MessageRef};

/// Gmail encodes bodies as base64url, usually unpadded; be lenient.
pub fn decode_base64url(data: &str) -> Vec<u8> {
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&cleaned)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(&cleaned))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&cleaned))
        .unwrap_or_default()
}

fn header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

#[derive(Default)]
struct Walk {
    text: Option<String>,
    html: Option<String>,
    attachments: Vec<AttachmentMeta>,
}

fn walk(part: &Part, out: &mut Walk) {
    let mime = part
        .mime_type
        .clone()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let filename = part.filename.clone().unwrap_or_default();
    if mime.starts_with("multipart/") {
        for p in &part.parts {
            walk(p, out);
        }
        return;
    }
    let inline_text = filename.is_empty() && part.body.attachment_id.is_none();
    if mime == "text/plain" && inline_text {
        if let Some(d) = &part.body.data {
            let s = String::from_utf8_lossy(&decode_base64url(d)).into_owned();
            match &mut out.text {
                Some(t) => {
                    t.push('\n');
                    t.push_str(&s);
                }
                None => out.text = Some(s),
            }
        }
        return;
    }
    if mime == "text/html" && inline_text {
        if let Some(d) = &part.body.data {
            let s = String::from_utf8_lossy(&decode_base64url(d)).into_owned();
            match &mut out.html {
                Some(h) => h.push_str(&s),
                None => out.html = Some(s),
            }
        }
        return;
    }
    if mime.is_empty()
        && part.parts.is_empty()
        && part.body.data.is_none()
        && part.body.attachment_id.is_none()
    {
        return;
    }
    let content_id = header(&part.headers, "Content-ID")
        .map(|c| c.trim_matches(|ch| ch == '<' || ch == '>').to_string());
    let filename = if filename.is_empty() {
        match mime.as_str() {
            "text/calendar" => "invite.ics".to_string(),
            _ => format!("part-{}", part.part_id.clone().unwrap_or_default()),
        }
    } else {
        filename
    };
    out.attachments.push(AttachmentMeta {
        filename,
        mime_type: mime,
        size_bytes: part.body.size,
        provider_id: part.body.attachment_id.clone(),
        content_id,
    });
}

/// A `users.messages.get` response (metadata or full) to a `Message`.
pub fn message_from_json(dig: &DigId, m: &GmailMessage) -> Message {
    let payload = m.payload.clone().unwrap_or_default();
    let h = &payload.headers;
    let date = m
        .internal_date
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(chrono::DateTime::from_timestamp_millis)
        .or_else(|| {
            header(h, "Date")
                .and_then(|d| chrono::DateTime::parse_from_rfc2822(d).ok())
                .map(|d| d.with_timezone(&chrono::Utc))
        })
        .unwrap_or_else(chrono::Utc::now);
    let mut walked = Walk::default();
    walk(&payload, &mut walked);
    let from = header(h, "From")
        .and_then(parse_address)
        .unwrap_or_default();
    let headers = MessageHeaders {
        message_id: header(h, "Message-ID")
            .or_else(|| header(h, "Message-Id"))
            .map(|s| s.trim().to_string()),
        in_reply_to: header(h, "In-Reply-To").map(|s| s.trim().to_string()),
        references: header(h, "References")
            .map(parse_message_ids)
            .unwrap_or_default(),
        list_id: header(h, "List-ID")
            .or_else(|| header(h, "List-Id"))
            .map(|s| s.trim().to_string()),
        list_unsubscribe: header(h, "List-Unsubscribe").map(|s| s.trim().to_string()),
        precedence: header(h, "Precedence").map(|s| s.trim().to_lowercase()),
        auto_submitted: header(h, "Auto-Submitted")
            .map(|s| s.trim().to_lowercase())
            .filter(|s| s != "no"),
        reply_to: header(h, "Reply-To").and_then(parse_address),
        authentication: header(h, "Authentication-Results")
            .map(parse_authentication_results)
            .unwrap_or_default(),
    };
    let has_body = walked.text.is_some() || walked.html.is_some();
    Message {
        id: MessageId::for_provider(dig, &m.id),
        dig_id: dig.clone(),
        thread_id: ThreadId::for_provider(dig, &m.thread_id),
        provider_id: m.id.clone(),
        subject: header(h, "Subject").map(decode_rfc2047).unwrap_or_default(),
        from,
        to: header(h, "To").map(parse_address_list).unwrap_or_default(),
        cc: header(h, "Cc").map(parse_address_list).unwrap_or_default(),
        bcc: header(h, "Bcc").map(parse_address_list).unwrap_or_default(),
        date,
        snippet: decode_entities(&m.snippet),
        labels: m.label_ids.clone(),
        headers,
        attachments: walked.attachments,
        size_bytes: m.size_estimate,
        is_sent: m.label_ids.iter().any(|l| l == "SENT"),
        has_body,
        body_text: walked.text,
        body_html: walked.html,
        synced_at: chrono::Utc::now(),
    }
}

pub fn labels_from_json(list: &LabelList) -> Vec<ProviderLabel> {
    list.labels
        .iter()
        .map(|l| ProviderLabel {
            id: l.id.clone(),
            name: l.name.clone(),
            managed_by_riddle: riddle_mail::is_managed_label_name(&l.name),
            system: l.kind.as_deref() == Some("system"),
        })
        .collect()
}

pub fn refs_from_list(list: &MessageList) -> Vec<MessageRef> {
    list.messages
        .iter()
        .map(|m| MessageRef {
            provider_id: m.id.clone(),
            thread_provider_id: m.thread_id.clone().unwrap_or_else(|| m.id.clone()),
        })
        .collect()
}

/// History records to changes, in order. A record with only `messages`
/// (no added/deleted/labels) carries nothing Riddle acts on.
pub fn changes_from_history(list: &HistoryList) -> Vec<Change> {
    let mut out = Vec::new();
    for r in &list.history {
        for a in &r.messages_added {
            out.push(Change::Added {
                id: a.message.id.clone(),
            });
        }
        for d in &r.messages_deleted {
            out.push(Change::Deleted {
                id: d.message.id.clone(),
            });
        }
        for l in &r.labels_added {
            out.push(Change::LabelsChanged {
                id: l.message.id.clone(),
                added: l.label_ids.clone(),
                removed: vec![],
            });
        }
        for l in &r.labels_removed {
            out.push(Change::LabelsChanged {
                id: l.message.id.clone(),
                added: vec![],
                removed: l.label_ids.clone(),
            });
        }
    }
    out
}

/// What a Workspace user should ask their administrator for (§16).
pub fn admin_blocked_message(client_id: &str) -> String {
    format!(
        "Your Google Workspace administrator has not allowed Riddle to access Gmail. Ask them to allow the OAuth client {client_id} \
         in the Google Admin console (Security → Access and data control → API controls → Manage third-party app access), \
         marking it Trusted or granting the gmail.modify scope."
    )
}

/// Map an HTTP failure to the taxonomy. `history` says the request was
/// `history.list`, where 404 means the cursor expired rather than "not found".
pub fn map_http_error(
    status: u16,
    retry_after_secs: Option<u64>,
    body: &str,
    history: bool,
    client_id: &str,
) -> RiddleError {
    let envelope: Option<ErrorEnvelope> = serde_json::from_str(body).ok();
    let (reasons, message, oauth_code): (Vec<String>, String, Option<String>) =
        match envelope.as_ref().and_then(|e| e.error.clone()) {
            Some(serde_json::Value::Object(_)) => {
                let api: ApiErrorBody = envelope
                    .as_ref()
                    .and_then(|e| e.error.clone())
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or(ApiErrorBody {
                        code: None,
                        message: None,
                        errors: vec![],
                        status: None,
                    });
                (
                    api.errors.iter().filter_map(|e| e.reason.clone()).collect(),
                    api.message.unwrap_or_default(),
                    None,
                )
            }
            Some(serde_json::Value::String(code)) => (
                vec![],
                envelope
                    .as_ref()
                    .and_then(|e| e.error_description.clone())
                    .unwrap_or_default(),
                Some(code),
            ),
            _ => (vec![], body.chars().take(200).collect(), None),
        };
    let lower = body.to_ascii_lowercase();
    if oauth_code.as_deref() == Some("admin_policy_enforced")
        || reasons.iter().any(|r| r == "admin_policy_enforced")
        || lower.contains("admin_policy_enforced")
    {
        return RiddleError::new(
            ErrorKind::ProviderAdminBlocked,
            admin_blocked_message(client_id),
        );
    }
    if let Some(code) = oauth_code {
        return match code.as_str() {
            "invalid_grant" => {
                RiddleError::provider_auth("Gmail sign-in expired or was revoked; sign in again")
            }
            "invalid_client" | "unauthorized_client" => RiddleError::provider_auth(
                "the Gmail OAuth client id or secret is wrong; check settings",
            ),
            "access_denied" => RiddleError::provider_auth("Gmail access was declined"),
            _ => RiddleError::provider_auth(format!("Gmail sign-in failed ({code}): {message}")),
        };
    }
    let rate_reasons = [
        "rateLimitExceeded",
        "userRateLimitExceeded",
        "dailyLimitExceeded",
        "quotaExceeded",
        "RATE_LIMIT_EXCEEDED",
    ];
    if status == 429 || reasons.iter().any(|r| rate_reasons.contains(&r.as_str())) {
        let secs = retry_after_secs.unwrap_or(60);
        return RiddleError::rate_limited(format!("Gmail rate limit; retry after {secs}s"))
            .with_detail(serde_json::json!({"retryAfterSeconds": secs}));
    }
    match status {
        401 => RiddleError::provider_auth("Gmail rejected the credential; sign in again"),
        403 => {
            if reasons
                .iter()
                .any(|r| r == "insufficientPermissions" || r == "forbidden")
                || lower.contains("insufficient")
            {
                RiddleError::provider_auth(format!(
                    "Gmail refused: {message}. Sign in again to grant the gmail.modify scope"
                ))
            } else if lower.contains("mail service not enabled")
                || lower.contains("failedprecondition")
            {
                RiddleError::new(
                    ErrorKind::ProviderAdminBlocked,
                    admin_blocked_message(client_id),
                )
            } else {
                RiddleError::provider(format!("Gmail refused the request: {message}"))
            }
        }
        404 if history => RiddleError::history_expired(
            "the Gmail history cursor has expired; a full resync is required",
        ),
        404 => RiddleError::not_found(format!("Gmail: {message}")),
        400 => RiddleError::invalid(format!("Gmail rejected the request: {message}")),
        s if s >= 500 => RiddleError::provider(format!("Gmail server error {s}: {message}")),
        s => RiddleError::provider(format!("Gmail HTTP {s}: {message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::AuthVerdict;

    const FULL: &str = include_str!("../fixtures/message_full.json");
    const META: &str = include_str!("../fixtures/message_metadata.json");
    const HISTORY: &str = include_str!("../fixtures/history.json");
    const LABELS: &str = include_str!("../fixtures/labels.json");
    const LIST: &str = include_str!("../fixtures/message_list.json");
    const RATE: &str = include_str!("../fixtures/error_rate_limit.json");
    const ADMIN: &str = include_str!("../fixtures/error_admin_policy.json");

    #[test]
    fn a_full_message_yields_headers_bodies_attachments_and_a_calendar_part() {
        let raw: GmailMessage = serde_json::from_str(FULL).unwrap();
        let dig = DigId::from("d1");
        let m = message_from_json(&dig, &raw);
        assert_eq!(m.id.as_str(), "d1:18f2a9c0b1d2e3f4");
        assert_eq!(m.thread_id.provider_id(), "18f2a9c0b1d2e3f0");
        assert_eq!(
            m.subject,
            "Re: [example/riddle] Add outbox processor (PR #142)"
        );
        assert_eq!(m.from.email, "notifications@github.com");
        assert_eq!(m.from.name.as_deref(), Some("Priya Natarajan"));
        assert_eq!(m.cc.len(), 2);
        assert_eq!(m.cc[1].email, "review_requested@noreply.github.com");
        assert_eq!(m.date.to_rfc3339(), "2026-09-21T12:02:00+00:00");
        assert_eq!(
            m.snippet,
            "@priya requested your review on: #142 Add outbox processor & friends"
        );
        assert_eq!(
            m.labels,
            vec!["INBOX", "UNREAD", "CATEGORY_UPDATES", "Label_7"]
        );
        assert_eq!(
            m.headers.list_id.as_deref(),
            Some("example/riddle <riddle.example.github.com>")
        );
        assert_eq!(m.headers.precedence.as_deref(), Some("list"));
        assert!(m.headers.list_unsubscribe.is_some());
        assert_eq!(
            m.headers.in_reply_to.as_deref(),
            Some("<example/riddle/pull/142@github.com>")
        );
        assert_eq!(m.headers.references.len(), 1);
        assert_eq!(
            m.headers.reply_to.as_ref().unwrap().email,
            "reply+abcdef@reply.github.com"
        );
        assert_eq!(m.headers.authentication.spf, AuthVerdict::Pass);
        assert_eq!(m.headers.authentication.dmarc, AuthVerdict::Pass);
        assert_eq!(
            m.body_text.as_deref(),
            Some("@priya requested your review on: #142 Add outbox processor & friends.")
        );
        assert_eq!(
            m.body_html.as_deref(),
            Some("<p>@priya requested your review on: <b>#142</b></p>")
        );
        assert!(m.has_body);
        assert_eq!(m.attachments.len(), 2);
        assert_eq!(m.attachments[0].mime_type, "text/calendar");
        assert_eq!(
            m.attachments[0].provider_id.as_deref(),
            Some("ANGjdJ8calendar")
        );
        assert_eq!(
            m.attachments[1].content_id.as_deref(),
            Some("logo@example.com")
        );
        assert!(riddle_mail::normalize::has_calendar_part(&m));
        assert!(!m.is_sent);
        assert_eq!(m.size_bytes, 9411);
    }

    #[test]
    fn a_metadata_message_has_no_body_and_is_sent_when_labelled_so() {
        let raw: GmailMessage = serde_json::from_str(META).unwrap();
        let m = message_from_json(&DigId::from("d1"), &raw);
        assert!(m.is_sent);
        assert!(!m.has_body);
        assert!(m.body_text.is_none());
        assert_eq!(m.to[0].email, "dan@example.com");
        assert!(m.attachments.is_empty());
    }

    #[test]
    fn history_records_become_ordered_changes() {
        let list: HistoryList = serde_json::from_str(HISTORY).unwrap();
        let changes = changes_from_history(&list);
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0], Change::Added { id: "m1".into() });
        assert_eq!(
            changes[1],
            Change::LabelsChanged {
                id: "m2".into(),
                added: vec![],
                removed: vec!["INBOX".into()]
            }
        );
        assert_eq!(
            changes[2],
            Change::LabelsChanged {
                id: "m2".into(),
                added: vec!["Label_7".into()],
                removed: vec![]
            }
        );
        assert_eq!(changes[3], Change::Deleted { id: "m3".into() });
        assert_eq!(list.history_id.as_deref(), Some("4471110"));
    }

    #[test]
    fn labels_and_listings_parse_with_the_managed_marker() {
        let labels = labels_from_json(&serde_json::from_str(LABELS).unwrap());
        assert_eq!(labels.len(), 5);
        assert!(labels[0].system && !labels[0].managed_by_riddle);
        let spoil = labels.iter().find(|l| l.id == "Label_7").unwrap();
        assert!(spoil.managed_by_riddle && !spoil.system);
        assert!(
            !labels
                .iter()
                .find(|l| l.id == "Label_9")
                .unwrap()
                .managed_by_riddle
        );
        let list: MessageList = serde_json::from_str(LIST).unwrap();
        let refs = refs_from_list(&list);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].thread_provider_id, "18f2a9c0b1d2e3f0");
        assert_eq!(list.next_page_token.as_deref(), Some("09876543210"));
    }

    #[test]
    fn errors_map_to_the_taxonomy() {
        let e = map_http_error(403, None, RATE, false, "cid");
        assert_eq!(e.kind, ErrorKind::ProviderRateLimited);
        assert_eq!(e.detail.unwrap()["retryAfterSeconds"], 60);
        let e = map_http_error(429, Some(7), "", false, "cid");
        assert_eq!(e.kind, ErrorKind::ProviderRateLimited);
        assert_eq!(e.detail.unwrap()["retryAfterSeconds"], 7);
        let e = map_http_error(403, None, ADMIN, false, "cid-123");
        assert_eq!(e.kind, ErrorKind::ProviderAdminBlocked);
        assert!(e.message.contains("cid-123") && e.message.contains("Admin console"));
        assert_eq!(
            map_http_error(401, None, "", false, "cid").kind,
            ErrorKind::ProviderAuth
        );
        assert_eq!(
            map_http_error(
                404,
                None,
                "{\"error\":{\"code\":404,\"message\":\"Requested entity was not found.\"}}",
                true,
                "cid"
            )
            .kind,
            ErrorKind::ProviderHistoryExpired
        );
        assert_eq!(
            map_http_error(404, None, "", false, "cid").kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            map_http_error(503, None, "", false, "cid").kind,
            ErrorKind::Provider
        );
        assert_eq!(map_http_error(400, None, "{\"error\":\"invalid_grant\",\"error_description\":\"Token has been expired or revoked.\"}", false, "cid").kind, ErrorKind::ProviderAuth);
        assert_eq!(map_http_error(403, None, "{\"error\":{\"code\":403,\"message\":\"Insufficient Permission\",\"errors\":[{\"reason\":\"insufficientPermissions\"}]}}", false, "cid").kind, ErrorKind::ProviderAuth);
    }

    #[test]
    fn base64url_decoding_is_lenient_about_padding_and_alphabet() {
        assert_eq!(decode_base64url("aGVsbG8"), b"hello");
        assert_eq!(decode_base64url("aGVsbG8="), b"hello");
        assert_eq!(decode_base64url("PHA-"), b"<p>");
        assert_eq!(decode_base64url("PHA+"), b"<p>");
    }
}
