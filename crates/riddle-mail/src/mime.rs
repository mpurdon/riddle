//! Header-level MIME helpers shared by providers: address lists, RFC 2047
//! encoded words, `Authentication-Results`, and building an RFC 5322 message
//! for sending. Hand-written rather than a full parser dependency because
//! the provider (Gmail) already gives structured parts; only headers arrive
//! as raw text.

use base64::Engine;
use riddle_core::{Address, AuthVerdict, AuthenticationResults};

/// Parse `Name <a@b>, "Last, First" <c@d>, e@f` into addresses. Tolerant:
/// anything unparseable is dropped rather than failing the message.
pub fn parse_address_list(raw: &str) -> Vec<Address> {
    let mut out = Vec::new();
    for part in split_outside_quotes(raw, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(a) = parse_address(part) {
            out.push(a);
        }
    }
    out
}

/// Parse a single mailbox.
pub fn parse_address(raw: &str) -> Option<Address> {
    let raw = raw.trim().trim_end_matches(';');
    if let Some(open) = raw.rfind('<') {
        let close = raw[open..].find('>').map(|c| open + c).unwrap_or(raw.len());
        let email = raw[open + 1..close].trim();
        if !looks_like_email(email) {
            return None;
        }
        let name = decode_rfc2047(raw[..open].trim())
            .trim()
            .trim_matches('"')
            .trim()
            .to_string();
        let name = if name.is_empty() { None } else { Some(name) };
        return Some(Address::new(email, name.as_deref()));
    }
    let bare = raw.trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>');
    if looks_like_email(bare) {
        return Some(Address::new(bare, None));
    }
    None
}

fn looks_like_email(s: &str) -> bool {
    let Some((local, domain)) = s.rsplit_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.contains(' ') && !local.contains(' ')
}

fn split_outside_quotes(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            '<' if !in_quotes => {
                depth += 1;
                cur.push(c);
            }
            '>' if !in_quotes => {
                depth -= 1;
                cur.push(c);
            }
            c if c == sep && !in_quotes && depth <= 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Decode `=?charset?B?...?=` and `=?charset?Q?...?=` words. Only UTF-8 and
/// Latin-1 are honoured; other charsets fall back to lossy UTF-8.
pub fn decode_rfc2047(s: &str) -> String {
    if !s.contains("=?") {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    let mut last_was_word = false;
    while let Some(start) = rest.find("=?") {
        let (before, from_start) = rest.split_at(start);
        if !(last_was_word && before.trim().is_empty()) {
            out.push_str(before);
        }
        let Some(end) = from_start[2..].find("?=").map(|e| e + 2) else {
            out.push_str(from_start);
            return out;
        };
        let word = &from_start[2..end];
        let mut parts = word.splitn(3, '?');
        let charset = parts.next().unwrap_or("").to_ascii_lowercase();
        let enc = parts.next().unwrap_or("").to_ascii_uppercase();
        let payload = parts.next().unwrap_or("");
        let bytes: Vec<u8> = match enc.as_str() {
            "B" => base64::engine::general_purpose::STANDARD
                .decode(payload)
                .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(payload))
                .unwrap_or_default(),
            "Q" => decode_q(payload),
            _ => payload.as_bytes().to_vec(),
        };
        let text = if charset.starts_with("iso-8859-1") || charset == "latin1" {
            bytes.iter().map(|&b| b as char).collect()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        out.push_str(&text);
        rest = &from_start[end + 2..];
        last_was_word = true;
    }
    out.push_str(rest);
    out
}

fn decode_q(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => out.push(b' '),
            b'=' => {
                if let (Some(h), Some(l)) = (
                    hex_val(b.get(i + 1).copied()),
                    hex_val(b.get(i + 2).copied()),
                ) {
                    out.push(h << 4 | l);
                    i += 2;
                } else {
                    out.push(b'=');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

fn hex_val(c: Option<u8>) -> Option<u8> {
    match c? {
        c @ b'0'..=b'9' => Some(c - b'0'),
        c @ b'a'..=b'f' => Some(c - b'a' + 10),
        c @ b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Encode a header value as an RFC 2047 B word when it is not plain ASCII.
pub fn encode_header_word(s: &str) -> String {
    if s.is_ascii() && !s.contains("=?") {
        s.to_string()
    } else {
        format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
        )
    }
}

/// `Authentication-Results: mx.google.com; dkim=pass ...; spf=pass ...; dmarc=fail ...`
pub fn parse_authentication_results(raw: &str) -> AuthenticationResults {
    let lower = raw.to_ascii_lowercase();
    let verdict = |key: &str| -> AuthVerdict {
        let mut best = AuthVerdict::None;
        for seg in lower.split(';') {
            let seg = seg.trim();
            if let Some(rest) = seg.strip_prefix(key) {
                if let Some(v) = rest.strip_prefix('=') {
                    let v = v
                        .split(|c: char| c.is_whitespace() || c == '(')
                        .next()
                        .unwrap_or("");
                    let this = match v {
                        "pass" => AuthVerdict::Pass,
                        "fail" | "softfail" | "hardfail" | "permerror" => AuthVerdict::Fail,
                        _ => AuthVerdict::None,
                    };
                    // Any failing segment counts as a failure.
                    if this == AuthVerdict::Fail || best == AuthVerdict::None {
                        best = this;
                    }
                }
            }
        }
        best
    };
    AuthenticationResults {
        spf: verdict("spf"),
        dkim: verdict("dkim"),
        dmarc: verdict("dmarc"),
    }
}

/// Split a `References:` header into ids.
pub fn parse_message_ids(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn format_address(a: &Address) -> String {
    match &a.name {
        Some(n) if !n.is_empty() => format!("{} <{}>", quote_name(n), a.email),
        _ => a.email.clone(),
    }
}

fn quote_name(n: &str) -> String {
    let enc = encode_header_word(n);
    if enc.starts_with("=?")
        || !n
            .chars()
            .any(|c| matches!(c, ',' | '<' | '>' | '"' | ';' | ':'))
    {
        enc
    } else {
        format!("\"{}\"", n.replace('"', "'"))
    }
}

/// Build an RFC 5322 message with a text part and an optional HTML
/// alternative. The `Message-ID` is generated here so a reply can reference
/// it before the provider echoes it back.
pub fn build_rfc822(
    m: &crate::provider::OutgoingMessage,
    message_id: &str,
    date: chrono::DateTime<chrono::Utc>,
) -> String {
    let mut s = String::new();
    s.push_str(&format!("From: {}\r\n", format_address(&m.from)));
    if !m.to.is_empty() {
        s.push_str(&format!(
            "To: {}\r\n",
            m.to.iter()
                .map(format_address)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !m.cc.is_empty() {
        s.push_str(&format!(
            "Cc: {}\r\n",
            m.cc.iter()
                .map(format_address)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !m.bcc.is_empty() {
        s.push_str(&format!(
            "Bcc: {}\r\n",
            m.bcc
                .iter()
                .map(format_address)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    s.push_str(&format!("Subject: {}\r\n", encode_header_word(&m.subject)));
    s.push_str(&format!("Date: {}\r\n", date.to_rfc2822()));
    s.push_str(&format!("Message-ID: {message_id}\r\n"));
    if let Some(r) = &m.in_reply_to {
        s.push_str(&format!("In-Reply-To: {r}\r\n"));
    }
    if !m.references.is_empty() {
        s.push_str(&format!("References: {}\r\n", m.references.join(" ")));
    }
    s.push_str("MIME-Version: 1.0\r\n");
    match &m.body_html {
        Some(html) => {
            let boundary = format!(
                "=_riddle_{}",
                &message_id
                    .trim_matches(|c| c == '<' || c == '>')
                    .replace('@', "_")
            );
            s.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n"
            ));
            s.push_str(&format!("--{boundary}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n"));
            s.push_str(&m.body_text.replace('\n', "\r\n"));
            s.push_str(&format!("\r\n--{boundary}\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n"));
            s.push_str(&html.replace('\n', "\r\n"));
            s.push_str(&format!("\r\n--{boundary}--\r\n"));
        }
        None => {
            s.push_str("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n");
            s.push_str(&m.body_text.replace('\n', "\r\n"));
            s.push_str("\r\n");
        }
    }
    s
}

/// A fresh RFC 5322 Message-ID under the sender's domain.
pub fn new_message_id(from_email: &str) -> String {
    let domain = from_email
        .rsplit_once('@')
        .map(|(_, d)| d)
        .unwrap_or("riddle.local");
    format!("<{}@{}>", uuid::Uuid::now_v7(), domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_lists_with_quoted_commas_and_encoded_names_parse() {
        let list = parse_address_list("\"Reyes, Sofia\" <sofia@example.com>, dan@example.com, =?UTF-8?B?UHLDrHlh?= <priya@example.com>");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name.as_deref(), Some("Reyes, Sofia"));
        assert_eq!(list[0].email, "sofia@example.com");
        assert_eq!(list[1].name, None);
        assert_eq!(list[2].name.as_deref(), Some("Prìya"));
    }

    #[test]
    fn q_encoded_words_decode_underscores_and_hex() {
        assert_eq!(decode_rfc2047("=?UTF-8?Q?Caf=C3=A9_menu?="), "Café menu");
        assert_eq!(decode_rfc2047("plain"), "plain");
    }

    #[test]
    fn authentication_results_take_any_failure_seriously() {
        let a = parse_authentication_results("mx.google.com; dkim=pass header.i=@x; spf=fail (sender ip) smtp.mailfrom=x; dmarc=pass");
        assert_eq!(a.spf, AuthVerdict::Fail);
        assert_eq!(a.dkim, AuthVerdict::Pass);
        assert_eq!(a.dmarc, AuthVerdict::Pass);
    }

    #[test]
    fn an_outgoing_message_becomes_a_well_formed_rfc822_text() {
        let m = crate::provider::OutgoingMessage {
            from: Address::new("me@example.com", Some("Matt")),
            to: vec![Address::new("you@example.com", None)],
            subject: "Café ☕".into(),
            body_text: "hello\nworld".into(),
            in_reply_to: Some("<abc@example.com>".into()),
            ..Default::default()
        };
        let raw = build_rfc822(&m, "<id@example.com>", chrono::Utc::now());
        assert!(raw.starts_with("From: Matt <me@example.com>\r\n"));
        assert!(raw.contains("Subject: =?UTF-8?B?"));
        assert!(raw.contains("In-Reply-To: <abc@example.com>"));
        assert!(raw.ends_with("hello\r\nworld\r\n"));
    }
}
