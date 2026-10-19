//! The model catalog: tiers and prices (§56). No model id is hard-coded into
//! a call site; the tiers here are defaults the settings file overrides, and
//! the pricing table is the only place a dollar figure lives.
//!
//! Prices are USD per million tokens as published for the Anthropic models
//! on 2026-09-22. A model without a verified price is unpriced and its calls
//! are reported as unpriced, never as free. Sonnet 5, Opus 5 and Fable were
//! not verified against the Bedrock price list tonight and stay unpriced
//! until someone reads the list; guessing a price is worse than none.
//!
//! Tier evidence:
//! - escalation on Haiku 4.5: the refiner asks a bounded question with strict
//!   JSON output at temperature 0; Haiku is the cheapest model that follows
//!   the contract, and escalation volume is the cost tail (§18, §39).
//! - Director on Sonnet 5: open-ended tool use over many turns; the tier the
//!   build plan names as the default pending the user's decision.
//! - verification on Haiku 4.5: cheap second opinions in Signal tests.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    Escalation,
    Director,
    Expedition,
    Verification,
}

/// USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// Cache reads are billed at 10% of input; writes at 125% (Anthropic's
    /// published multipliers for prompt caching).
    pub cache_read_per_mtok: f64,
    pub cache_write_per_mtok: f64,
}

impl ModelPrice {
    pub const fn new(input: f64, output: f64) -> Self {
        Self {
            input_per_mtok: input,
            output_per_mtok: output,
            cache_read_per_mtok: input * 0.1,
            cache_write_per_mtok: input * 1.25,
        }
    }
    pub fn cost(&self, input: u64, output: u64, cache_read: u64, cache_write: u64) -> f64 {
        let m = 1_000_000.0;
        (input as f64 / m) * self.input_per_mtok
            + (output as f64 / m) * self.output_per_mtok
            + (cache_read as f64 / m) * self.cache_read_per_mtok
            + (cache_write as f64 / m) * self.cache_write_per_mtok
    }
}

/// A catalog entry: the substring that identifies a model family in a
/// provider model id, and its price if verified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub family: String,
    pub label: String,
    pub price: Option<ModelPrice>,
}

#[derive(Debug, Clone)]
pub struct ModelCatalog {
    entries: Vec<CatalogEntry>,
    tiers: HashMap<ModelTier, String>,
}

pub const HAIKU_4_5: &str = "us.anthropic.claude-haiku-4-5-20251001-v1:0";
pub const SONNET_5: &str = "us.anthropic.claude-sonnet-5";
/// The Director default: accessible on the owner's account and priced. Sonnet 5
/// returned AccessDenied on 2026-09-22; switch back once access is granted.
pub const SONNET_4_5: &str = "us.anthropic.claude-sonnet-4-5-20250929-v1:0";
pub const OPUS_5: &str = "us.anthropic.claude-opus-5";

impl Default for ModelCatalog {
    fn default() -> Self {
        let entries = vec![
            CatalogEntry {
                family: "claude-haiku-4-5".into(),
                label: "Claude Haiku 4.5".into(),
                price: Some(ModelPrice::new(1.0, 5.0)),
            },
            CatalogEntry {
                family: "claude-sonnet-4-5".into(),
                label: "Claude Sonnet 4.5".into(),
                price: Some(ModelPrice::new(3.0, 15.0)),
            },
            CatalogEntry {
                family: "claude-opus-4".into(),
                label: "Claude Opus 4.x".into(),
                price: Some(ModelPrice::new(15.0, 75.0)),
            },
            CatalogEntry {
                family: "claude-sonnet-5".into(),
                label: "Claude Sonnet 5".into(),
                price: None,
            },
            CatalogEntry {
                family: "claude-opus-5".into(),
                label: "Claude Opus 5".into(),
                price: None,
            },
            CatalogEntry {
                family: "claude-fable".into(),
                label: "Claude Fable".into(),
                price: None,
            },
        ];
        let mut tiers = HashMap::new();
        tiers.insert(ModelTier::Escalation, HAIKU_4_5.to_string());
        tiers.insert(ModelTier::Verification, HAIKU_4_5.to_string());
        tiers.insert(ModelTier::Director, SONNET_4_5.to_string());
        tiers.insert(ModelTier::Expedition, SONNET_4_5.to_string());
        Self { entries, tiers }
    }
}

impl ModelCatalog {
    pub fn new(entries: Vec<CatalogEntry>, tiers: HashMap<ModelTier, String>) -> Self {
        Self { entries, tiers }
    }

    pub fn set_tier(&mut self, tier: ModelTier, model_id: impl Into<String>) {
        self.tiers.insert(tier, model_id.into());
    }

    pub fn tier(&self, tier: ModelTier) -> Option<&str> {
        self.tiers.get(&tier).map(|s| s.as_str())
    }

    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Add or replace a price; the settings file feeds this.
    pub fn set_price(&mut self, family: &str, label: &str, price: Option<ModelPrice>) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.family == family) {
            e.price = price;
            e.label = label.to_string();
        } else {
            self.entries.push(CatalogEntry {
                family: family.into(),
                label: label.into(),
                price,
            });
        }
    }

    /// The entry whose family substring appears in the model id; longest
    /// match wins so `claude-opus-4` does not claim `claude-opus-4-5-x` when a
    /// more specific family exists.
    pub fn entry_for(&self, model_id: &str) -> Option<&CatalogEntry> {
        self.entries
            .iter()
            .filter(|e| model_id.contains(&e.family))
            .max_by_key(|e| e.family.len())
    }

    pub fn price(&self, model_id: &str) -> Option<ModelPrice> {
        self.entry_for(model_id).and_then(|e| e.price)
    }

    pub fn is_priced(&self, model_id: &str) -> bool {
        self.price(model_id).is_some()
    }

    /// `Some` only when the model is priced.
    pub fn cost(
        &self,
        model_id: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
    ) -> Option<f64> {
        self.price(model_id)
            .map(|p| p.cost(input, output, cache_read, cache_write))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priced_models_get_a_cost_and_unpriced_models_report_none() {
        let c = ModelCatalog::default();
        let haiku = c.cost(HAIKU_4_5, 1_000_000, 1_000_000, 0, 0).unwrap();
        assert!((haiku - 6.0).abs() < 1e-9);
        assert_eq!(c.cost(SONNET_5, 1000, 1000, 0, 0), None);
        assert_eq!(
            c.cost("global.anthropic.claude-opus-5", 1000, 1000, 0, 0),
            None
        );
        assert_eq!(c.cost("something-unknown", 1000, 1000, 0, 0), None);
    }

    #[test]
    fn cache_reads_are_cheaper_and_writes_dearer_than_input() {
        let p = ModelPrice::new(1.0, 5.0);
        assert!((p.cache_read_per_mtok - 0.1).abs() < 1e-9);
        assert!((p.cache_write_per_mtok - 1.25).abs() < 1e-9);
        assert!((p.cost(0, 0, 1_000_000, 0) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn tiers_default_to_haiku_for_escalation_and_sonnet_for_the_director() {
        let c = ModelCatalog::default();
        assert_eq!(c.tier(ModelTier::Escalation), Some(HAIKU_4_5));
        assert_eq!(c.tier(ModelTier::Director), Some(SONNET_4_5));
    }

    #[test]
    fn the_most_specific_family_wins() {
        let mut c = ModelCatalog::default();
        c.set_price(
            "claude-opus-4-1",
            "Opus 4.1",
            Some(ModelPrice::new(15.0, 75.0)),
        );
        assert_eq!(
            c.entry_for("us.anthropic.claude-opus-4-1-20250805-v1:0")
                .unwrap()
                .family,
            "claude-opus-4-1"
        );
    }
}
