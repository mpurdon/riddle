//! riddle-settings: the config file, the state file and the Actions that
//! change them (ARCHITECTURE.md §12, §51, §59).
//!
//! Three files with three owners: `settings.toml` is declarative and
//! hand-editable, `state.json` is the app's own runtime state, and anything
//! high-churn lives in the store. Both files are written atomically with
//! every field defaulted, because a settings save that clobbers live data,
//! or a launch that resets preferences after a half-written file, is the
//! kind of incident users remember. Secrets are never here: only presence
//! booleans derived through `SecretPresence`.

pub mod actions;
pub mod atomic;
pub mod model;
pub mod secrets;
pub mod store;

pub use actions::{actions, normalize_entry, ArmControl};
pub use model::*;
pub use secrets::{FixedPresence, NoSecrets, Secret, SecretPresence};
pub use store::{
    load_settings, load_state, parse_settings, SettingsStore, SETTINGS_FILE, STATE_FILE,
};
