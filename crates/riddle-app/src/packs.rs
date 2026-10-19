//! Detect-driven rule packs: which packs the mail on hand says apply, and
//! the install that runs when an account's first listing completes.
//!
//! Every built-in pack has carried a `[detect]` block that nothing read: a
//! new account got no rules until its owner found the catalogue, and the
//! packs every mailbox needs (Core, Accounts: `always = true`) were
//! installed by hand (17 Oct 2026). Now the always packs are installed,
//! Active, when an account's first listing completes (`PassHandler` in
//! `ops.rs`), because a new account's rules must decide and Observe
//! autonomy already keeps Gmail untouched. The rest are offers on the
//! Summary, each with "Turn it on" (the `install_pack` Action) and "Not
//! now" (`dismiss_pack_offer`), remembered per pack in the index store so
//! the offer is not made twice.

use crate::App;
use riddle_core::*;
use riddle_errors::Result;
use riddle_mesh::packs::{self, InstallReport, PackDetection};
use riddle_observability::{rinfo, swallowed, Category};
use riddle_store::Store;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// How far back detection looks when the sync has no window: the GitHub
/// pack asks for ten messages, and a year of them from a project long gone
/// quiet would not be mail worth rules.
const DETECT_WINDOW_DAYS: i64 = 120;

/// What `install_always_packs` did, pack by pack, for the ledger and undo.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlwaysInstall {
    pub installed: Vec<String>,
    pub reports: BTreeMap<String, InstallReport>,
}

/// The `always` packs not yet installed (catalogue order).
pub fn always_packs_pending(index: &Store) -> Result<Vec<packs::Pack>> {
    let installed: HashSet<String> = packs::installed(index)?.into_iter().map(|p| p.id).collect();
    Ok(packs::all_packs(index)?
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| p.detect.always && !installed.contains(&p.id))
        .collect())
}

/// Install every `always` pack not yet installed, Active, with its default
/// lists. A fresh store already holds the default Meshes by name, so the
/// pack takes them over (`replace`): a Mesh whose rules are unchanged
/// keeps its status, a new one starts Active. Idempotent.
pub fn install_always_packs(index: &Arc<Store>, by: CreatedBy) -> Result<AlwaysInstall> {
    let mut out = AlwaysInstall::default();
    for pack in always_packs_pending(index)? {
        let report = packs::install(index, &pack, &BTreeMap::new(), MeshStatus::Active, true, by)?;
        out.reports.insert(pack.id.clone(), report);
        out.installed.push(pack.id);
    }
    Ok(out)
}

/// Pack offers for a set of accounts, computed once and kept until the
/// mail or the packs change: the Summary computed them on every read, two
/// count queries per Dig over the detection window (19 Oct 2026). Every
/// sync job end and every pack install, removal, import or decline clears
/// it (`forget_offers`), so the answer is exact and the hot path is one
/// lookup.
#[derive(Default)]
pub struct PackOfferCache(parking_lot::Mutex<HashMap<Vec<DigId>, Vec<PackOffer>>>);

impl PackOfferCache {
    fn get(&self, only: &[DigId]) -> Option<Vec<PackOffer>> {
        self.0.lock().get(only).cloned()
    }

    fn put(&self, only: Vec<DigId>, offers: Vec<PackOffer>) {
        self.0.lock().insert(only, offers);
    }

    pub fn clear(&self) {
        self.0.lock().clear();
    }
}

/// The mail or the packs changed: the next Summary computes its offers.
pub fn forget_offers(services: &riddle_actions::Services) {
    if let Some(cache) = services.get::<PackOfferCache>() {
        cache.clear();
    }
}

impl App {
    /// Whether a completed first listing turns the always packs on
    /// (`Settings::install_packs_on_first_sync`; on unless the settings
    /// say otherwise, as test boots do).
    pub fn install_packs_on_first_sync(&self) -> bool {
        self.services
            .get::<riddle_settings::SettingsStore>()
            .is_none_or(|s| s.settings().install_packs_on_first_sync)
    }

    /// Where detection starts: the sync window's start when the sync has
    /// one, else `DETECT_WINDOW_DAYS` ago.
    fn detect_since(&self) -> chrono::DateTime<chrono::Utc> {
        let days = self
            .sync_options()
            .window_days
            .unwrap_or(DETECT_WINDOW_DAYS);
        self.services.clock.now() - chrono::Duration::days(days)
    }

    /// Packs the mail of these accounts (every enabled one when `only` is
    /// empty) matches, not installed and not declined, largest match
    /// first. A pack several accounts match is offered once, with the
    /// largest account's count, so the sentence and its number agree.
    pub fn pack_offers(&self, only: &[DigId]) -> Result<Vec<PackOffer>> {
        let index = self.stores.index();
        let cache = self.services.get::<PackOfferCache>();
        if let Some(offers) = cache.as_ref().and_then(|c| c.get(only)) {
            return Ok(offers);
        }
        let since = self.detect_since();
        let mut by_pack: BTreeMap<String, PackDetection> = BTreeMap::new();
        for dig in self.list_digs()? {
            if !only.is_empty() && !only.contains(&dig.id) {
                continue;
            }
            let store = self.stores.dig(&dig.id)?;
            for d in packs::offers(&index, &store, &since)? {
                match by_pack.get(&d.pack_id) {
                    Some(seen) if seen.matched_messages >= d.matched_messages => {}
                    _ => {
                        by_pack.insert(d.pack_id.clone(), d);
                    }
                }
            }
        }
        let mut offers: Vec<PackOffer> = by_pack
            .into_values()
            .map(|d| PackOffer {
                pack_id: d.pack_id,
                name: d.name,
                reason_in_words: d.reason_in_words,
                matched_messages: d.matched_messages,
            })
            .collect();
        offers.sort_by(|a, b| {
            b.matched_messages
                .cmp(&a.matched_messages)
                .then_with(|| a.name.cmp(&b.name))
        });
        if let Some(c) = &cache {
            c.put(only.to_vec(), offers.clone());
        }
        Ok(offers)
    }

    /// The install that runs when an account's first listing completes:
    /// the always packs, Active, under the System actor, with an Activity
    /// row naming the packs. Bookkeeping: a failure is reported and the
    /// sync that triggered it still succeeds; the next first listing, or a
    /// person, can run `install_detected_packs` again.
    pub fn install_packs_after_first_listing(&self, dig: &Dig) {
        forget_offers(&self.services);
        match install_always_packs(&self.stores.index(), CreatedBy::System) {
            Ok(done) if done.installed.is_empty() => {}
            Ok(done) => {
                rinfo!(
                    Category::Actions,
                    "installed {} always packs after the first listing of {}",
                    done.installed.len(),
                    dig.id
                );
                let mut note = Activity::note(
                    Actor::System,
                    "install_detected_packs",
                    format!(
                        "Turned on the rules every mailbox starts with: {}",
                        done.installed.join(", ")
                    ),
                    serde_json::json!({ "packIds": done.installed, "digId": dig.id }),
                    "a pack can be removed from its page under Rules",
                    None,
                    self.services.clock.now(),
                );
                note.dig_id = Some(dig.id.clone());
                self.runtime.record_activity(note);
                self.events.emit(RiddleEvent::SettingsChanged);
            }
            Err(e) => swallowed!(
                Category::Actions,
                "install_detected_packs",
                e,
                "packs not installed after the first listing of {}",
                dig.id
            ),
        }
    }
}
