//! `SettingsStore`: the one owner of `settings.toml` and `state.json`.
//!
//! Loading merges the file over the build profile's defaults at the TOML
//! level, so a field missing from an old file takes the dev default in a dev
//! build and the prod default in a prod build; serde's own defaults remain
//! the last line of defence. Saving validates, stamps the schema version,
//! writes atomically, then notifies: a `watch` channel for in-process
//! subscribers (the log filter, the job scheduler's LLM rate) and
//! `RiddleEvent::SettingsChanged` for the webview.

use crate::atomic::write_atomic;
use crate::model::{Settings, State, SETTINGS_SCHEMA_VERSION, STATE_SCHEMA_VERSION};
use crate::secrets::{Secret, SecretPresence};
use parking_lot::{Mutex, RwLock};
use riddle_actions::policy::AutonomyPolicy;
use riddle_core::{EventSink, RiddleEvent};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::watch;

pub const SETTINGS_FILE: &str = "settings.toml";
pub const STATE_FILE: &str = "state.json";

pub struct SettingsStore {
    config_path: PathBuf,
    state_path: PathBuf,
    dev: bool,
    secrets: Arc<dyn SecretPresence>,
    events: Arc<dyn EventSink>,
    settings: RwLock<Settings>,
    state: RwLock<State>,
    tx: watch::Sender<Settings>,
    /// Saves are serialized so two writers cannot interleave validate/write.
    save_lock: Mutex<()>,
}

impl std::fmt::Debug for SettingsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsStore")
            .field("config_path", &self.config_path)
            .field("dev", &self.dev)
            .finish()
    }
}

impl SettingsStore {
    /// Load (or default) both files. `config_dir` holds `settings.toml`,
    /// `data_dir` holds `state.json`; a dev build passes `dev = true` so
    /// missing fields take dev defaults (§59).
    pub fn open(
        config_dir: &Path,
        data_dir: &Path,
        dev: bool,
        secrets: Arc<dyn SecretPresence>,
        events: Arc<dyn EventSink>,
    ) -> Result<Self> {
        let config_path = config_dir.join(SETTINGS_FILE);
        let state_path = data_dir.join(STATE_FILE);
        let mut settings = load_settings(&config_path, dev)?;
        apply_presence(&mut settings, secrets.as_ref());
        let state = load_state(&state_path)?;
        let (tx, _rx) = watch::channel(settings.clone());
        rinfo!(
            Category::App,
            "settings loaded (schema v{}, dev={dev})",
            settings.schema_version
        );
        Ok(Self {
            config_path,
            state_path,
            dev,
            secrets,
            events,
            settings: RwLock::new(settings),
            state: RwLock::new(state),
            tx,
            save_lock: Mutex::new(()),
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
    pub fn state_path(&self) -> &Path {
        &self.state_path
    }
    pub fn is_dev(&self) -> bool {
        self.dev
    }
    pub fn defaults(&self) -> Settings {
        Settings::defaults_for(self.dev)
    }

    /// The current settings with presence flags derived now.
    pub fn settings(&self) -> Settings {
        let mut s = self.settings.read().clone();
        apply_presence(&mut s, self.secrets.as_ref());
        s
    }

    /// Validate, stamp, write atomically, notify. Returns the effective
    /// settings (presence flags refreshed).
    pub fn save(&self, mut next: Settings) -> Result<Settings> {
        let _guard = self.save_lock.lock();
        next.validate()?;
        next.schema_version = SETTINGS_SCHEMA_VERSION;
        let text = next.to_toml_string()?;
        write_atomic(&self.config_path, text.as_bytes())?;
        apply_presence(&mut next, self.secrets.as_ref());
        *self.settings.write() = next.clone();
        let _ = self.tx.send(next.clone());
        self.events.emit(RiddleEvent::SettingsChanged);
        rinfo!(Category::App, "settings saved");
        Ok(next)
    }

    /// Edit in place: `f` sees the current settings and the result is saved.
    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut next = self.settings();
        f(&mut next);
        self.save(next)
    }

    /// Subscribers see every saved value; the receiver starts at the current one.
    pub fn subscribe(&self) -> watch::Receiver<Settings> {
        self.tx.subscribe()
    }

    /// The policy for the Action runtime. `armed` is preserved by the caller.
    pub fn autonomy_policy(&self) -> AutonomyPolicy {
        let mut p = self.settings.read().autonomy_policy();
        p.dev = self.dev;
        p
    }

    pub fn state(&self) -> State {
        self.state.read().clone()
    }

    pub fn save_state(&self, mut next: State) -> Result<()> {
        let _guard = self.save_lock.lock();
        next.schema_version = STATE_SCHEMA_VERSION;
        let text = serde_json::to_string_pretty(&next)?;
        write_atomic(&self.state_path, text.as_bytes())?;
        *self.state.write() = next;
        Ok(())
    }

    pub fn update_state(&self, f: impl FnOnce(&mut State)) -> Result<()> {
        let mut next = self.state();
        f(&mut next);
        self.save_state(next)
    }
}

fn apply_presence(s: &mut Settings, secrets: &dyn SecretPresence) {
    s.judge.api_key_present = secrets.is_present(&Secret::judge(&s.judge));
    s.gmail.client_secret_present = secrets.is_present(&Secret::GmailClientSecret);
}

/// Parse the file over the profile defaults. Unknown keys are ignored; a
/// newer schema version is loaded with a warning rather than refused, since
/// refusing would reset the user's choices on a downgrade.
pub fn load_settings(path: &Path, dev: bool) -> Result<Settings> {
    let defaults = Settings::defaults_for(dev);
    if !path.exists() {
        return Ok(defaults);
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|e| RiddleError::internal(format!("read {}: {e}", path.display())))?;
    parse_settings(&raw, defaults).map_err(|e| {
        RiddleError::invalid(format!(
            "{} is not valid settings ({}); fix or rename it to reset",
            path.display(),
            e.message
        ))
    })
}

/// Merge `raw` over `defaults` and deserialize.
pub fn parse_settings(raw: &str, defaults: Settings) -> Result<Settings> {
    let file: toml::Value = toml::from_str(raw).map_err(|e| RiddleError::invalid(e.to_string()))?;
    let mut base =
        toml::Value::try_from(&defaults).map_err(|e| RiddleError::internal(e.to_string()))?;
    merge(&mut base, file);
    let settings: Settings = base
        .try_into()
        .map_err(|e: toml::de::Error| RiddleError::invalid(e.to_string()))?;
    if settings.schema_version > SETTINGS_SCHEMA_VERSION {
        rwarn!(
            Category::App,
            "settings schema v{} is newer than this build's v{}",
            settings.schema_version,
            SETTINGS_SCHEMA_VERSION
        );
    }
    Ok(settings)
}

/// Deep merge: tables recurse, everything else is replaced by the file.
fn merge(base: &mut toml::Value, over: toml::Value) {
    match (base, over) {
        (toml::Value::Table(b), toml::Value::Table(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(existing) if existing.is_table() && v.is_table() => merge(existing, v),
                    _ => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (b, o) => *b = o,
    }
}

pub fn load_state(path: &Path) -> Result<State> {
    if !path.exists() {
        return Ok(State::default());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|e| RiddleError::internal(format!("read {}: {e}", path.display())))?;
    match serde_json::from_str::<State>(&raw) {
        Ok(s) => Ok(s),
        Err(e) => {
            // State is reconstructible; a corrupt file must not block launch.
            rwarn!(
                Category::App,
                "state.json unreadable ({e}); starting from defaults"
            );
            Ok(State::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{FixedPresence, NoSecrets};
    use riddle_core::{Autonomy, RecordingSink};

    fn open(dir: &Path, dev: bool) -> SettingsStore {
        SettingsStore::open(
            &dir.join("config"),
            &dir.join("data"),
            dev,
            Arc::new(NoSecrets),
            Arc::new(RecordingSink::default()),
        )
        .unwrap()
    }

    #[test]
    fn a_missing_file_yields_profile_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            open(dir.path(), true).settings().autonomy.default,
            Autonomy::Observe
        );
        assert_eq!(
            open(dir.path(), false).settings().autonomy.default,
            Autonomy::Suggest
        );
    }

    #[test]
    fn a_partial_file_takes_the_profile_defaults_for_missing_fields() {
        let raw = "developerMode = true\n[budgets]\ndailyLlmUsd = 2.5\n";
        let dev = parse_settings(raw, Settings::dev_defaults()).unwrap();
        assert!(dev.developer_mode);
        assert_eq!(dev.budgets.daily_llm_usd, 2.5);
        assert_eq!(dev.budgets.daily_judge_usd, 1.0);
        assert_eq!(dev.autonomy.default, Autonomy::Observe);
        let prod = parse_settings(raw, Settings::prod_defaults()).unwrap();
        assert_eq!(prod.autonomy.default, Autonomy::Suggest);
    }

    #[test]
    fn unknown_keys_are_ignored_not_fatal() {
        let raw = "futureFlag = true\n[judge]\nnewKnob = 3\n";
        let s = parse_settings(raw, Settings::prod_defaults()).unwrap();
        assert_eq!(s.judge.model, "jev-1.13.0");
    }

    #[test]
    fn save_round_trips_notifies_and_emits() {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(RecordingSink::default());
        let store = SettingsStore::open(
            &dir.path().join("config"),
            &dir.path().join("data"),
            false,
            Arc::new(NoSecrets),
            events.clone(),
        )
        .unwrap();
        let mut rx = store.subscribe();
        let saved = store
            .update(|s| {
                s.shadow_period_days = 3;
                s.autonomy.per_dig.insert("dig-1".into(), Autonomy::Act);
            })
            .unwrap();
        assert_eq!(saved.shadow_period_days, 3);
        assert!(rx.has_changed().unwrap());
        assert_eq!(rx.borrow_and_update().shadow_period_days, 3);
        assert!(matches!(
            events.take().as_slice(),
            [RiddleEvent::SettingsChanged]
        ));
        let again = open(dir.path(), false).settings();
        assert_eq!(again, saved);
    }

    #[test]
    fn an_invalid_save_leaves_the_file_and_memory_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let store = open(dir.path(), false);
        store.update(|s| s.shadow_period_days = 2).unwrap();
        let err = store
            .update(|s| s.budgets.daily_judge_usd = -3.0)
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        assert_eq!(store.settings().budgets.daily_judge_usd, 1.0);
        assert_eq!(open(dir.path(), false).settings().shadow_period_days, 2);
    }

    #[test]
    fn presence_flags_come_from_the_secret_source_not_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let present = Arc::new(FixedPresence {
            judge_key: true,
            gmail_secret: false,
        });
        let store = SettingsStore::open(
            &dir.path().join("c"),
            &dir.path().join("d"),
            false,
            present,
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
        let s = store.settings();
        assert!(s.judge.api_key_present);
        assert!(!s.gmail.client_secret_present);
        store.save(s).unwrap();
        let text = std::fs::read_to_string(store.config_path()).unwrap();
        assert!(!text.contains("Present"));
        assert!(!open(dir.path(), false).settings().judge.api_key_present);
    }

    #[test]
    fn state_is_separate_from_config_and_survives_a_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = open(dir.path(), false);
        store
            .update_state(|st| st.onboarding.completed = true)
            .unwrap();
        assert!(store.state_path().exists());
        assert!(!store.config_path().exists());
        assert!(open(dir.path(), false).state().onboarding.completed);
        std::fs::write(store.state_path(), "{not json").unwrap();
        assert!(!open(dir.path(), false).state().onboarding.completed);
    }

    #[test]
    fn the_policy_carries_the_build_profile() {
        let dir = tempfile::tempdir().unwrap();
        assert!(open(dir.path(), true).autonomy_policy().dev);
        assert!(!open(dir.path(), false).autonomy_policy().dev);
    }
}
