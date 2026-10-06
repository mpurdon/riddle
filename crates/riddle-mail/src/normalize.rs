//! Message text normalization for judgment: HTML to text, quoted-reply and
//! signature stripping, a size cap, language detection, and the two
//! structural facts that come from parts rather than words (calendar,
//! structured receipt).
//!
//! The judge sees the normalized text, so what is stripped here is what the
//! judge never sees. Quoted replies are stripped because a thread's history
//! is given to the judge as Thread State, not as five nested copies; a
//! signature is stripped because "Sent from my iPhone" is not evidence of
//! anything. Both strippers are conservative: when unsure, keep the text.

use regex::Regex;
use riddle_core::Message;
use std::sync::OnceLock;

/// Bytes of normalized text the judge is given.
pub const JUDGMENT_CAP_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedText {
    pub text: String,
    /// ISO 639-1, or "und".
    pub language: String,
    pub quoted_stripped: bool,
    pub signature_stripped: bool,
    pub truncated: bool,
    pub from_html: bool,
}

/// The text Sift judges: prefer the plain part, fall back to HTML, then the
/// snippet. Quotes and signatures stripped, capped.
pub fn normalize_message(m: &Message) -> NormalizedText {
    let (raw, from_html) = match (&m.body_text, &m.body_html) {
        (Some(t), _) if !t.trim().is_empty() => (t.clone(), false),
        (_, Some(h)) if !h.trim().is_empty() => (html_to_text(h), true),
        _ => (m.snippet.clone(), false),
    };
    normalize_text(&raw, from_html)
}

pub fn normalize_text(raw: &str, from_html: bool) -> NormalizedText {
    let collapsed = collapse_whitespace(raw);
    let (unquoted, quoted_stripped) = strip_quoted_reply(&collapsed);
    let (unsigned, signature_stripped) = strip_signature(&unquoted);
    let (text, truncated) = cap(&unsigned, JUDGMENT_CAP_BYTES);
    let language = detect_language(&text).to_string();
    NormalizedText {
        text,
        language,
        quoted_stripped,
        signature_stripped,
        truncated,
        from_html,
    }
}

/// Cut at a char boundary at or below `max` bytes.
pub fn cap(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    (text[..floor_char_boundary(text, max)].to_string(), true)
}

/// The largest char boundary at or below `max` bytes. Every byte cut of
/// mail text goes through this: a cut inside a multi-byte character panics.
pub use riddle_core::floor_char_boundary;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static regex"))
}

/// HTML to readable text: block tags become line breaks, scripts/styles are
/// dropped with their content, entities decoded, whitespace collapsed.
pub fn html_to_text(html: &str) -> String {
    static DROP: OnceLock<Regex> = OnceLock::new();
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static BR: OnceLock<Regex> = OnceLock::new();
    static INLINE: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    static COMMENT: OnceLock<Regex> = OnceLock::new();
    let s = re(&COMMENT, r"(?s)<!--.*?-->").replace_all(html, " ");
    let s = re(&DROP, r"(?is)<(script|style|head|title|template)\b[^>]*>.*?</\s*(script|style|head|title|template)\s*>").replace_all(&s, " ");
    let s = re(&BR, r"(?i)<br\s*/?>").replace_all(&s, "\n");
    let s = re(&BLOCK, r"(?i)(<(p|div|tr|li|ul|ol|h[1-6]|table|blockquote|pre|section|article|header|footer|hr)\b[^>]*>|</(p|div|tr|ul|ol|h[1-6]|table|blockquote|pre|section|article|header|footer)\s*>)").replace_all(&s, "\n");
    // Inline formatting tags vanish without a space so "<b>Matt</b>," stays "Matt,".
    let s = re(&INLINE, r"(?i)</?(b|i|u|s|em|strong|span|a|font|code|small|big|sup|sub|mark|abbr|q|cite|del|ins)\b[^>]*>").replace_all(&s, "");
    let s = re(&TAG, r"(?s)<[^>]*>").replace_all(&s, " ");
    let s = decode_entities(&s);
    collapse_whitespace(&s)
}

/// Decode the entities that appear in mail; unknown named entities are kept.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // Entities are short, so look for the `;` in the next 12 bytes, cut
        // on a char boundary: real mail puts accents and emoji right after a
        // bare `&`, and a byte cut inside one panicked the first Gmail sync.
        let window = floor_char_boundary(tail, 12);
        let Some(semi) = tail[..window].find(';') else {
            out.push('&');
            rest = &rest[i + 1..];
            continue;
        };
        let ent = &tail[1..semi];
        let decoded: Option<String> = if let Some(num) = ent.strip_prefix('#') {
            let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            code.and_then(char::from_u32).map(|c| c.to_string())
        } else {
            named_entity(ent).map(|c| c.to_string())
        };
        match decoded {
            Some(d) => {
                out.push_str(&d);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "hellip" => "…",
        "mdash" => "—",
        "ndash" => "–",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "bull" => "•",
        "middot" => "·",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "cent" => "¢",
        "deg" => "°",
        "times" => "×",
        "eacute" => "é",
        "egrave" => "è",
        "agrave" => "à",
        "ccedil" => "ç",
        "uuml" => "ü",
        "ouml" => "ö",
        "auml" => "ä",
        "szlig" => "ß",
        "ntilde" => "ñ",
        "zwnj" | "zwj" | "shy" => "",
        _ => return None,
    })
}

/// Collapse runs of spaces and more than two consecutive newlines; trim lines.
pub fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0;
    for line in s.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Remove quoted earlier messages: Gmail's `On … wrote:` (possibly wrapped),
/// Outlook's `From:/Sent:/To:/Subject:` block, `-----Original Message-----`,
/// and `>`-prefixed lines. Returns whether anything was removed.
pub fn strip_quoted_reply(text: &str) -> (String, bool) {
    static ON_WROTE: OnceLock<Regex> = OnceLock::new();
    static ON_START: OnceLock<Regex> = OnceLock::new();
    static ORIGINAL: OnceLock<Regex> = OnceLock::new();
    static OUTLOOK_FROM: OnceLock<Regex> = OnceLock::new();
    static OUTLOOK_NEXT: OnceLock<Regex> = OnceLock::new();
    static FORWARDED: OnceLock<Regex> = OnceLock::new();
    let on_wrote = re(
        &ON_WROTE,
        r"(?i)^(On|Le|Am|El|Em)\b.{0,200}\b(wrote|a écrit|schrieb|escribió|escreveu)\s*:\s*$",
    );
    let on_start = re(
        &ON_START,
        r"(?i)^(On|Le|Am|El|Em)\s+(Mon|Tue|Wed|Thu|Fri|Sat|Sun|\d{1,2}|[A-Z][a-z]{2,8})\b",
    );
    let original = re(
        &ORIGINAL,
        r"(?i)^-{2,}\s*(Original Message|Forwarded message|Mensaje original|Message d'origine|Ursprüngliche Nachricht)\s*-{2,}\s*$",
    );
    let outlook_from = re(&OUTLOOK_FROM, r"(?i)^(\*\s*)?From:\s*\S.*$");
    let outlook_next = re(
        &OUTLOOK_NEXT,
        r"(?i)^(\*\s*)?(Sent|To|Date|Subject|Cc):\s*.*$",
    );
    let forwarded = re(&FORWARDED, r"^_{10,}\s*$");

    let lines: Vec<&str> = text.lines().collect();
    let mut cut: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim();
        if on_wrote.is_match(l) || original.is_match(l) || forwarded.is_match(l) {
            cut = Some(i);
            break;
        }
        // "On Tue, Sep 16, 2026 at 3:02 PM Priya <p@x>" wrapped onto a second line ending in "wrote:".
        if on_start.is_match(l) && i + 1 < lines.len() {
            let joined = format!("{} {}", l, lines[i + 1].trim());
            if on_wrote.is_match(&joined) {
                cut = Some(i);
                break;
            }
        }
        if outlook_from.is_match(l)
            && i + 1 < lines.len()
            && outlook_next.is_match(lines[i + 1].trim())
        {
            cut = Some(i);
            break;
        }
    }
    let kept: Vec<&str> = match cut {
        Some(c) => lines[..c].to_vec(),
        None => lines.clone(),
    };
    let before_len = kept.len();
    let unquoted: Vec<&str> = kept
        .into_iter()
        .filter(|l| !l.trim_start().starts_with('>'))
        .collect();
    let stripped = cut.is_some() || unquoted.len() != before_len;
    (collapse_whitespace(&unquoted.join("\n")), stripped)
}

/// Remove a trailing signature: the `-- ` delimiter, or a sign-off line
/// ("Thanks,", "Best regards,") within the last eight lines, or a mobile
/// footer. The body above a sign-off is kept.
pub fn strip_signature(text: &str) -> (String, bool) {
    static SIGNOFF: OnceLock<Regex> = OnceLock::new();
    static MOBILE: OnceLock<Regex> = OnceLock::new();
    let signoff = re(
        &SIGNOFF,
        r"(?i)^(thanks|thank you|many thanks|thanks again|best|best regards|kind regards|warm regards|regards|cheers|sincerely|yours|talk soon|take care|all the best|cordialement|saludos|gracias|mit freundlichen grüßen|viele grüße|atenciosamente|obrigado)\s*[,!.]?\s*$",
    );
    let mobile = re(
        &MOBILE,
        r"(?i)^(sent from my |get outlook for |sent via |enviado desde mi |envoyé de mon )",
    );
    let lines: Vec<&str> = text.lines().collect();
    if let Some(i) = lines.iter().position(|l| *l == "-- " || *l == "--") {
        return (collapse_whitespace(&lines[..i].join("\n")), true);
    }
    let n = lines.len();
    let window_start = n.saturating_sub(8);
    // The earliest sign-off in the window wins, so "Thanks,\nPriya\nSent from
    // my iPhone" loses all three lines.
    for (i, line) in lines.iter().enumerate().take(n).skip(window_start) {
        let l = line.trim();
        if signoff.is_match(l) || mobile.is_match(l) {
            // Do not strip the whole message when the sign-off is the only content.
            if i == 0 {
                break;
            }
            return (collapse_whitespace(&lines[..i].join("\n")), true);
        }
    }
    (text.to_string(), false)
}

/// Stopword scoring across en/es/fr/de/pt. English unless another language
/// is plain: mail with real English in it is English, including the half
/// English, half French mail governments in Canada send, and a message is
/// called something else only on strong evidence. Links and addresses are
/// not words: a sale email with fifty `….com/…` links and no sentences was
/// "Portuguese" on the strength of `com`, and Sift held every one of them
/// for escalation (742 messages on the mailbox that showed it).
pub fn detect_language(text: &str) -> &'static str {
    const EN: &[&str] = &[
        "the", "and", "you", "that", "for", "with", "this", "have", "are", "not", "your", "from",
        "will", "can", "please", "thanks", "would", "about", "which", "what",
    ];
    const ES: &[&str] = &[
        "que", "los", "las", "para", "con", "por", "una", "del", "este", "esta", "como", "pero",
        "más", "gracias", "hola", "usted", "también", "puede", "sobre", "nos",
    ];
    const FR: &[&str] = &[
        "les", "des", "est", "pour", "vous", "dans", "une", "que", "pas", "sur", "avec", "nous",
        "cette", "merci", "bonjour", "votre", "sont", "être", "plus", "aussi",
    ];
    const DE: &[&str] = &[
        "und", "der", "die", "das", "nicht", "ich", "sie", "mit", "für", "ist", "auf", "ein",
        "eine", "wir", "danke", "bitte", "haben", "sind", "werden", "auch",
    ];
    const PT: &[&str] = &[
        "que", "não", "para", "com", "uma", "por", "você", "mais", "como", "obrigado", "isso",
        "são", "está", "também", "seu", "sua", "nós", "muito", "pelo", "ela",
    ];
    let is_link = |t: &str| {
        t.contains("://")
            || t.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("www."))
            || t.contains('@')
            || t.split('.').filter(|p| !p.is_empty()).count() >= 2
                && t.chars().any(|c| c == '.' || c == '/')
                && !t.ends_with('.')
    };
    let mut scores = [0usize; 5];
    let mut words = 0usize;
    for w in text
        .split_whitespace()
        .filter(|t| !is_link(t))
        .flat_map(|t| t.split(|c: char| !c.is_alphabetic()))
        .filter(|w| !w.is_empty())
    {
        let w = w.to_lowercase();
        words += 1;
        for (i, list) in [EN, ES, FR, DE, PT].iter().enumerate() {
            if list.contains(&w.as_str()) {
                scores[i] += 1;
            }
        }
        if words > 2000 {
            break;
        }
    }
    let (best_i, best) = scores
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| **s)
        .unwrap_or((0, &0));
    let english = scores[0];
    if words < 3 || (english >= 3 && english * 10 >= *best * 3) {
        return "en";
    }
    // Strong evidence only: five hits, and twice the runner-up (Spanish and
    // Portuguese share "que"/"para"; French and Spanish share "que").
    let mut sorted = scores;
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    if *best < 5 || *best < sorted[1] * 2 {
        return "en";
    }
    ["en", "es", "fr", "de", "pt"][best_i]
}

/// A `text/calendar` part or an `.ics` attachment, or an inline VCALENDAR.
pub fn has_calendar_part(m: &Message) -> bool {
    m.attachments.iter().any(|a| {
        a.mime_type.eq_ignore_ascii_case("text/calendar")
            || a.mime_type.eq_ignore_ascii_case("application/ics")
            || a.filename.to_ascii_lowercase().ends_with(".ics")
    }) || m
        .body_text
        .as_deref()
        .is_some_and(|t| t.contains("BEGIN:VCALENDAR"))
}

/// schema.org Order markup, or receipt vocabulary in the subject or body.
pub fn has_structured_receipt(m: &Message) -> bool {
    static KEYWORDS: OnceLock<Regex> = OnceLock::new();
    let kw = re(
        &KEYWORDS,
        r"(?i)\b(order (number|no\.?|#)|order confirmation|your order|receipt|invoice (number|no\.?|#)|invoice for|total charged|amount paid|payment received|thank you for your (purchase|order|payment))\b",
    );
    if let Some(h) = &m.body_html {
        let lower = h.to_ascii_lowercase();
        if lower.contains("schema.org/order")
            || lower.contains("schema.org/invoice")
            || lower.contains("\"@type\": \"order\"")
            || lower.contains("\"@type\":\"order\"")
            || lower.contains("\"@type\": \"invoice\"")
            || lower.contains("\"@type\":\"invoice\"")
        {
            return true;
        }
    }
    kw.is_match(&m.subject)
        || m.body_text
            .as_deref()
            .is_some_and(|t| kw.is_match(&t[..floor_char_boundary(t, 4000)]))
        || kw.is_match(&m.snippet)
}

/// "urgent", "immediately", "within 24 hours", "account will be suspended".
pub fn urgency_language_present(text: &str) -> bool {
    static URGENT: OnceLock<Regex> = OnceLock::new();
    re(&URGENT, r"(?i)\b(urgent(ly)?|immediately|right away|within (24|48) hours|act now|final (notice|warning)|will be (suspended|closed|terminated|deleted)|last chance|expires? (today|tonight|soon)|asap)\b").is_match(text)
}

/// Asks for a password, credentials, a card number, a wire, or a gift card.
pub fn asks_for_payment_or_credentials(text: &str) -> bool {
    static ASK: OnceLock<Regex> = OnceLock::new();
    re(&ASK, r"(?i)\b(verify your (account|identity|password)|confirm your (password|credentials|account)|enter your (password|credentials|card)|update your (payment|billing) (information|details)|wire transfer|gift cards?|social security|bank details|re-?enter your password|login credentials)\b").is_match(text)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_byte_cut_never_lands_inside_a_character() {
        let t = "é".repeat(3000);
        let end = floor_char_boundary(&t, 4001);
        assert_eq!(end, 4000);
        assert_eq!(floor_char_boundary("ab", 10), 2);
    }

    #[test]
    fn an_ampersand_before_accented_text_does_not_split_a_character() {
        assert_eq!(decode_entities("Tom & Jérôme fêté"), "Tom & Jérôme fêté");
        assert_eq!(decode_entities("&ééééééé;"), "&ééééééé;");
        assert_eq!(
            decode_entities("R&D 🚀🚀🚀 &amp; more"),
            "R&D 🚀🚀🚀 & more"
        );
    }

    use super::*;

    #[test]
    fn html_becomes_readable_text_with_entities_decoded_and_scripts_dropped() {
        let html = "<html><head><style>p{}</style><title>T</title></head><body><p>Hello&nbsp;<b>Matt</b>,</p><script>alert(1)</script><div>Total: &pound;12 &amp; more &#169;</div><br><ul><li>one</li><li>two</li></ul></body></html>";
        let text = html_to_text(html);
        assert_eq!(text, "Hello Matt,\n\nTotal: £12 & more ©\n\none\ntwo");
    }

    #[test]
    fn a_gmail_style_quoted_reply_is_stripped() {
        let text = "Sounds good, I'll send the numbers Friday.\n\nOn Tue, Sep 15, 2026 at 3:02 PM Priya Natarajan <priya@example.com> wrote:\n> Can you send me the Q3 numbers?\n> Thanks";
        let (out, stripped) = strip_quoted_reply(text);
        assert!(stripped);
        assert_eq!(out, "Sounds good, I'll send the numbers Friday.");
    }

    #[test]
    fn a_wrapped_on_wrote_line_is_stripped() {
        let text = "Yes.\n\nOn Tue, Sep 15, 2026 at 3:02 PM Priya Natarajan\n<priya@example.com> wrote:\n\n> Ready?";
        let (out, stripped) = strip_quoted_reply(text);
        assert!(stripped);
        assert_eq!(out, "Yes.");
    }

    #[test]
    fn an_outlook_style_reply_block_is_stripped() {
        let text = "Approved.\n\nFrom: Dan Whitfield <dan@example.com>\nSent: Monday, September 14, 2026 9:00 AM\nTo: Matt <matt@example.com>\nSubject: Budget\n\nPlease approve the budget.";
        let (out, stripped) = strip_quoted_reply(text);
        assert!(stripped);
        assert_eq!(out, "Approved.");
        let (out2, s2) =
            strip_quoted_reply("Here.\n\n-----Original Message-----\nFrom: x\nold text");
        assert!(s2);
        assert_eq!(out2, "Here.");
    }

    #[test]
    fn a_from_line_alone_is_not_treated_as_a_quote() {
        let text = "From: the top of the mountain you can see the sea.\nIt was lovely.";
        let (out, stripped) = strip_quoted_reply(text);
        assert!(!stripped);
        assert_eq!(out, text);
    }

    #[test]
    fn signatures_are_stripped_at_the_delimiter_or_the_sign_off() {
        let (a, sa) = strip_signature("Body here.\n-- \nMatt\nExample Inc");
        assert!(sa);
        assert_eq!(a, "Body here.");
        let (b, sb) =
            strip_signature("Can you review this?\n\nThanks,\nPriya\nSent from my iPhone");
        assert!(sb);
        assert_eq!(b, "Can you review this?");
        let (c, sc) = strip_signature("Thanks!");
        assert!(!sc);
        assert_eq!(c, "Thanks!");
    }

    #[test]
    fn language_detection_recognises_the_supported_languages_and_defaults_to_english() {
        assert_eq!(
            detect_language("Hola, gracias por el mensaje, para nosotros esta bien que los envíes"),
            "es"
        );
        assert_eq!(
            detect_language("Bonjour, merci pour votre message, nous sommes dans les temps"),
            "fr"
        );
        assert_eq!(
            detect_language(
                "Hallo, danke für die Nachricht, wir sind mit dem Plan nicht einverstanden"
            ),
            "de"
        );
        assert_eq!(
            detect_language("Can you send the numbers for the report that we discussed?"),
            "en"
        );
        assert_eq!(detect_language("42"), "en");
    }

    #[test]
    fn links_are_not_words_and_a_sale_email_full_of_them_is_english() {
        let links = "https://firearmsoutletcanada.com/henry-rifles www.firearmsoutletcanada.com/sale sales@firearmsoutletcanada.com firearmsoutletcanada.com/tm22 ".repeat(15);
        let sale = format!("SURPLUS SUNDAY Has Arrived! TM22 $499.99 Henry Big Boy $1,249 {links}");
        assert_eq!(detect_language(&sale), "en");
    }

    #[test]
    fn bilingual_government_mail_is_english_and_other_languages_need_plain_evidence() {
        let both = "Your licence renewal is due. Please renew it before the date shown. \
                    Votre permis doit être renouvelé. Veuillez le renouveler avant la date indiquée, merci pour votre attention dans les délais.";
        assert_eq!(detect_language(both), "en");
        assert_eq!(
            detect_language("Bonjour, merci pour votre message. Nous sommes dans les temps pour la livraison, avec plus de détails dans cette note."),
            "fr"
        );
        assert_eq!(
            detect_language("que para una"),
            "en",
            "three stray words are not Spanish"
        );
    }

    #[test]
    fn the_cap_respects_char_boundaries() {
        let s = "héllo wörld";
        let (t, truncated) = cap(s, 2);
        assert!(truncated);
        assert_eq!(t, "h");
    }

    #[test]
    fn receipt_and_urgency_heuristics_fire_on_the_obvious_cases() {
        let mut m = crate::fixture::blank_message("d", "p1");
        m.subject = "Your order #112-3344 has shipped".into();
        assert!(has_structured_receipt(&m));
        m.subject = "Lunch?".into();
        assert!(!has_structured_receipt(&m));
        m.body_html = Some("<div itemscope itemtype=\"http://schema.org/Order\"></div>".into());
        assert!(has_structured_receipt(&m));
        assert!(urgency_language_present(
            "Your account will be suspended within 24 hours"
        ));
        assert!(asks_for_payment_or_credentials(
            "Please verify your account by entering your password"
        ));
    }
}
