//! Process-wide state handed to every command.

use crate::paths::{AppDirs, Profile};
use parking_lot::Mutex;
use riddle_core::BuildStamp;
use riddle_store::Stores;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

pub struct AppState {
    pub app: tauri::AppHandle,
    pub dirs: AppDirs,
    pub profile: Profile,
    pub build: BuildStamp,
    pub started_at: Instant,
    pub stores: Arc<Stores>,
    /// In-flight command names with their start, for the Developer pane.
    pub in_flight: Mutex<HashMap<u64, (String, Instant)>>,
    /// Set by `riddle-actions` integration; commands look the registry up here.
    pub runtime: Mutex<Option<Arc<riddle_actions::Runtime>>>,
}

impl AppState {
    pub fn bootstrap(app: tauri::AppHandle, dirs: AppDirs, profile: Profile) -> Result<Self, Box<dyn std::error::Error>> {
        let build = BuildStamp::from_env_vars(
            env!("CARGO_PKG_VERSION"),
            option_env!("RIDDLE_GIT_SHA"),
            option_env!("RIDDLE_GIT_DIRTY"),
            option_env!("RIDDLE_BUILT_AT"),
            profile.as_str(),
        );
        let stores = Arc::new(Stores::open(&dirs.data)?);
        Ok(Self {
            app,
            dirs,
            profile,
            build,
            started_at: Instant::now(),
            stores,
            in_flight: Mutex::new(HashMap::new()),
            runtime: Mutex::new(None),
        })
    }

    pub fn runtime(&self) -> Result<Arc<riddle_actions::Runtime>, riddle_errors::RiddleError> {
        self.runtime.lock().clone().ok_or_else(|| riddle_errors::RiddleError::internal("runtime not initialised"))
    }
}
