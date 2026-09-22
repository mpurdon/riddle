//! Reversible redaction of personal identifiers before Context leaves the
//! device (§57). Placeholders are stable within one call (the same address
//! always maps to the same `[email-n]`) so the model can still follow who
//! said what, and the map restores the originals for display.
//!
//! The name heuristic is deliberately simple: two or more adjacent
//! capitalised words. It over-redacts ("Google Cloud" becomes a person) and
//! under-redacts single names, which is the right trade for a courtesy
//! boundary: the vendor's terms, not this regex, are the privacy guarantee.

use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The placeholder to original mapping for one redaction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactionMap {
    /// (placeholder, original) in first-seen order.
    entries: Vec<(String, String)>,
}

impl RedactionMap {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }
    pub fn original(&self, placeholder: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(p, _)| p == placeholder)
            .map(|(_, o)| o.as_str())
    }

    /// Put the originals back into `text` (or any text that quotes the
    /// placeholders, such as a model's answer).
    pub fn restore(&self, text: &str) -> String {
        let mut out = text.to_string();
        // Longer placeholders first so `[person-12]` is not clobbered by `[person-1]`.
        let mut entries: Vec<&(String, String)> = self.entries.iter().collect();
        entries.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        for (placeholder, original) in entries {
            out = out.replace(placeholder, original);
        }
        out
    }

    /// Merge another map so one map can restore a whole Context bundle.
    pub fn merge(&mut self, other: &RedactionMap) {
        for (p, o) in &other.entries {
            if !self.entries.iter().any(|(q, _)| q == p) {
                self.entries.push((p.clone(), o.clone()));
            }
        }
    }
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap())
}

fn phone_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // International or national forms with 7+ digits and common separators.
    RE.get_or_init(|| {
        Regex::new(
            r"(?:\+?\d{1,3}[\s.\-]?)?(?:\(\d{2,4}\)[\s.\-]?)?\d{3}[\s.\-]?\d{3,4}[\s.\-]?\d{0,4}",
        )
        .unwrap()
    })
}

fn name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Two to four adjacent capitalised words, letters only.
    RE.get_or_init(|| Regex::new(r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+){1,3}\b").unwrap())
}

/// A redactor that keeps placeholder numbering stable across several texts.
#[derive(Debug, Default)]
pub struct Redactor {
    map: RedactionMap,
    by_original: HashMap<String, String>,
    counters: HashMap<&'static str, usize>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    fn placeholder(&mut self, kind: &'static str, original: &str) -> String {
        let key = format!("{kind}:{}", original.to_lowercase());
        if let Some(p) = self.by_original.get(&key) {
            return p.clone();
        }
        let n = self.counters.entry(kind).or_insert(0);
        *n += 1;
        let p = format!("[{kind}-{n}]");
        self.by_original.insert(key, p.clone());
        self.map.entries.push((p.clone(), original.to_string()));
        p
    }

    /// Redact one text; placeholders continue numbering from earlier calls.
    pub fn redact(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        // Pass 1: addresses, so their local parts are never mistaken for names.
        let mut last = 0;
        for m in email_re().find_iter(text) {
            out.push_str(&text[last..m.start()]);
            let p = self.placeholder("email", m.as_str());
            out.push_str(&p);
            last = m.end();
        }
        out.push_str(&text[last..]);

        // Pass 2: phone numbers with at least seven digits.
        let text = out;
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for m in phone_re().find_iter(&text) {
            let digits = m.as_str().chars().filter(|c| c.is_ascii_digit()).count();
            if digits < 7 {
                continue;
            }
            out.push_str(&text[last..m.start()]);
            let p = self.placeholder("phone", m.as_str().trim());
            out.push_str(&p);
            last = m.end();
        }
        out.push_str(&text[last..]);

        // Pass 3: capitalised name pairs.
        let text = out;
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for m in name_re().find_iter(&text) {
            out.push_str(&text[last..m.start()]);
            let p = self.placeholder("person", m.as_str());
            out.push_str(&p);
            last = m.end();
        }
        out.push_str(&text[last..]);
        out
    }

    pub fn map(&self) -> &RedactionMap {
        &self.map
    }
    pub fn into_map(self) -> RedactionMap {
        self.map
    }
}

/// Replace addresses, phone numbers and capitalised name pairs with stable
/// placeholders; the map restores them.
pub fn redact(text: &str) -> (String, RedactionMap) {
    let mut r = Redactor::new();
    let out = r.redact(text);
    (out, r.into_map())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_is_reversible_and_placeholders_are_stable() {
        let text = "Ana Silva <ana.silva@example.com> called +1 415 555 0199 and again ana.silva@example.com; Ana Silva agreed.";
        let (redacted, map) = redact(text);
        assert!(!redacted.contains("ana.silva@example.com"));
        assert!(!redacted.contains("415"));
        assert!(!redacted.contains("Ana Silva"));
        assert_eq!(redacted.matches("[email-1]").count(), 2, "{redacted}");
        assert_eq!(redacted.matches("[person-1]").count(), 2, "{redacted}");
        assert!(redacted.contains("[phone-1]"));
        assert_eq!(map.restore(&redacted), text);
    }

    #[test]
    fn different_people_get_different_placeholders() {
        let (redacted, map) = redact("Ana Silva wrote to Ben Okoro.");
        assert_eq!(redacted, "[person-1] wrote to [person-2].");
        assert_eq!(map.original("[person-2]"), Some("Ben Okoro"));
    }

    #[test]
    fn short_digit_runs_are_not_phone_numbers() {
        let (redacted, map) = redact("Order 12345 ships in 3 days.");
        assert_eq!(redacted, "Order 12345 ships in 3 days.");
        assert!(map.is_empty());
    }

    #[test]
    fn restore_handles_more_than_nine_placeholders() {
        let mut r = Redactor::new();
        let names: Vec<String> = (0..12).map(|i| format!("Person{} Name{}", i, i)).collect();
        // Force distinct capitalised pairs.
        let people = [
            "Ana Silva",
            "Ben Okoro",
            "Cal Dune",
            "Dee Fox",
            "Eli Grey",
            "Fay Hill",
            "Gus Ito",
            "Hal Jun",
            "Ivy Kim",
            "Jon Lee",
            "Kim Moe",
            "Lou Nix",
        ];
        let _ = names;
        let text = people.join(", ");
        let redacted = r.redact(&text);
        assert!(redacted.contains("[person-12]"));
        assert_eq!(r.map().restore(&redacted), text);
    }

    #[test]
    fn a_redactor_keeps_numbering_across_texts_and_maps_merge() {
        let mut r = Redactor::new();
        let a = r.redact("From ana@example.com");
        let b = r.redact("To ben@example.com and ana@example.com");
        assert_eq!(a, "From [email-1]");
        assert_eq!(b, "To [email-2] and [email-1]");
        let mut merged = RedactionMap::default();
        merged.merge(r.map());
        assert_eq!(merged.len(), 2);
    }
}
