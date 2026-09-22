//! A local, dependency-free embedding for semantic retrieval (§45, §57
//! local-only mode). This is a hashed bag-of-words: tokens and adjacent
//! bigrams are feature-hashed (FNV-1a, sign from a second hash bit) into a
//! fixed 512-dimensional vector with sublinear term frequency and a static
//! down-weighting of stopwords standing in for IDF; vectors are
//! L2-normalised so cosine similarity is a dot product.
//!
//! It is good enough tonight for "where's that receipt for the access point"
//! and it is explicitly a placeholder: the `model` column names the scheme
//! (`hbow-512-v1`), `embed` is the one function to swap for a real model, and
//! `reindex_stale` re-embeds rows whose model differs from `MODEL`.
//! The `embeddings` table is owned by this crate (migration
//! `dig/0002_embeddings.sql`); nothing else reads or writes it.

use riddle_core::{DigId, Message, MessageId};
use riddle_errors::Result;
use riddle_observability::{rdebug, Category};
use riddle_store::rusqlite::params;
use riddle_store::{now_ts, repo, Store};

pub const DIM: usize = 512;
pub const MODEL: &str = "hbow-512-v1";

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "on", "for", "is", "are", "was", "were", "be",
    "been", "it", "this", "that", "with", "as", "at", "by", "from", "you", "your", "we", "our",
    "i", "me", "my", "he", "she", "they", "them", "his", "her", "its", "not", "no", "yes", "do",
    "does", "did", "have", "has", "had", "will", "would", "can", "could", "should", "may", "might",
    "if", "then", "so", "but", "than", "too", "very", "just", "about", "into", "over", "also",
    "any", "all", "here", "there", "when", "what", "which", "who", "how", "re", "fw", "fwd", "hi",
    "hello", "thanks", "thank", "regards", "best", "please", "email", "mail",
];

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Tokens: lowercase alphanumeric runs of 2+ chars, digits kept (order
/// numbers matter for receipts).
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

/// Embed a text into a unit vector. Empty text gives the zero vector.
pub fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; DIM];
    let tokens = tokenize(text);
    let mut counts: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for (i, t) in tokens.iter().enumerate() {
        *counts.entry(t.clone()).or_default() += 1.0;
        if i + 1 < tokens.len() {
            *counts.entry(format!("{t} {}", tokens[i + 1])).or_default() += 1.0;
        }
    }
    for (feature, count) in counts {
        let is_bigram = feature.contains(' ');
        let weight = if !is_bigram && STOPWORDS.contains(&feature.as_str()) {
            0.05
        } else if is_bigram {
            0.6
        } else {
            1.0
        };
        let tf = 1.0 + count.ln();
        let h = fnv1a(feature.as_bytes());
        let idx = (h % DIM as u64) as usize;
        let sign = if (h >> 63) & 1 == 1 { -1.0 } else { 1.0 };
        v[idx] += sign * weight * tf;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// The text a message is indexed on: subject, sender, attachment names, body.
pub fn index_text(m: &Message) -> String {
    let mut s = String::new();
    s.push_str(&m.subject);
    s.push('\n');
    s.push_str(&m.from.email);
    if let Some(n) = &m.from.name {
        s.push(' ');
        s.push_str(n);
    }
    for a in &m.attachments {
        s.push('\n');
        s.push_str(&a.filename);
    }
    s.push('\n');
    match &m.body_text {
        Some(t) => s.push_str(&t[..t.len().min(8000)]),
        None => s.push_str(&m.snippet),
    }
    s
}

pub fn index_message(store: &Store, m: &Message) -> Result<()> {
    let v = embed(&index_text(m));
    store.write(|c| {
        c.execute(
            "INSERT INTO embeddings (message_id, model, dim, vector, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(message_id) DO UPDATE SET model = excluded.model, dim = excluded.dim, vector = excluded.vector, updated_at = excluded.updated_at",
            params![m.id.as_str(), MODEL, DIM as i64, to_blob(&v), now_ts()],
        )?;
        Ok(())
    })
}

/// Ids not yet embedded under the current model, newest first.
pub fn pending_ids(store: &Store, limit: u32) -> Result<Vec<MessageId>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT m.id FROM messages m LEFT JOIN embeddings e ON e.message_id = m.id
             WHERE m.deleted_at IS NULL AND (e.message_id IS NULL OR e.model != ?1)
             ORDER BY m.date DESC LIMIT ?2",
        )?;
        let rows = st
            .query_map(params![MODEL, limit], |r| Ok(MessageId(r.get(0)?)))?
            .collect();
        rows
    })
}

/// Embed up to `limit` pending messages. Returns how many were indexed.
pub fn index_pending(store: &Store, dig: &DigId, limit: u32) -> Result<u64> {
    let ids = pending_ids(store, limit)?;
    let mut n = 0;
    for id in ids {
        if let Some(m) = repo::messages::get(store, dig, &id)? {
            index_message(store, &m)?;
            n += 1;
        }
    }
    if n > 0 {
        rdebug!(Category::Store, "embedded {} messages in dig {}", n, dig);
    }
    Ok(n)
}

pub fn indexed_count(store: &Store) -> Result<u64> {
    store.read(|c| {
        c.query_row(
            "SELECT count(*) FROM embeddings WHERE model = ?1",
            [MODEL],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as u64)
    })
}

/// The `k` most similar messages to a query, as (id, cosine), best first.
/// A full scan: at 2 KB per vector this is fine to a few hundred thousand
/// messages, and a real model would bring its own index.
pub fn semantic_search(store: &Store, query: &str, k: usize) -> Result<Vec<(MessageId, f32)>> {
    let q = embed(query);
    if q.iter().all(|x| *x == 0.0) {
        return Ok(vec![]);
    }
    let scored: Vec<(MessageId, f32)> = store.read(|c| {
        let mut st = c.prepare("SELECT e.message_id, e.vector FROM embeddings e JOIN messages m ON m.id = e.message_id WHERE e.model = ?1 AND m.deleted_at IS NULL")?;
        let rows = st.query_map([MODEL], |r| {
            let id: String = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            Ok((MessageId(id), from_blob(&blob)))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, v) = row?;
            let score: f32 = q.iter().zip(&v).map(|(a, b)| a * b).sum();
            if score > 0.0 {
                out.push((id, score));
            }
        }
        Ok(out)
    })?;
    let mut scored = scored;
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::Address;
    use riddle_mail::fixture::blank_message;
    use riddle_store::{Store, StoreKind};

    #[test]
    fn similar_texts_score_higher_than_unrelated_ones() {
        let a = embed("Your Amazon order of USB-C hub has shipped, order number 112-3344");
        let b = embed("Order confirmation: USB-C hub, arriving Thursday");
        let c = embed("Sprint review moved to Thursday afternoon, please update the agenda");
        assert!(cosine(&a, &b) > cosine(&a, &c));
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-5);
        assert_eq!(embed("").iter().filter(|x| **x != 0.0).count(), 0);
        assert_eq!(a.len(), DIM);
    }

    #[test]
    fn messages_are_indexed_and_a_receipt_is_found_by_meaning() {
        let store = Store::in_memory(StoreKind::Dig).unwrap();
        let dig = DigId::from("d");
        let mut receipt = blank_message("d", "r1");
        receipt.subject = "Your order #112-4455 of \"Wireless access point\"".into();
        receipt.from = Address::new("auto-confirm@amazon.example", Some("Amazon"));
        receipt.body_text =
            Some("Thank you for your order. Item: Wireless access point. Total $129.00".into());
        let mut meeting = blank_message("d", "m1");
        meeting.subject = "Invitation: Sprint review".into();
        meeting.body_text = Some(
            "You have been invited to the following event. Sprint review, Thursday 2pm.".into(),
        );
        let mut ci = blank_message("d", "c1");
        ci.subject = "[example/riddle] Run failed: CI - main".into();
        ci.body_text = Some("Workflow CI failed on main. View the run.".into());
        for m in [&receipt, &meeting, &ci] {
            repo::messages::upsert(&store, m).unwrap();
        }
        assert_eq!(pending_ids(&store, 10).unwrap().len(), 3);
        assert_eq!(index_pending(&store, &dig, 10).unwrap(), 3);
        assert_eq!(indexed_count(&store).unwrap(), 3);
        assert!(pending_ids(&store, 10).unwrap().is_empty());
        let hits = semantic_search(&store, "receipt for the access point order", 2).unwrap();
        assert_eq!(hits[0].0, receipt.id);
        let hits = semantic_search(&store, "CI failed on main", 1).unwrap();
        assert_eq!(hits[0].0, ci.id);
        assert!(semantic_search(&store, "", 5).unwrap().is_empty());
    }
}
