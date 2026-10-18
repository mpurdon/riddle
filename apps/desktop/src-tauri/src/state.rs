//! Process-wide state handed to every command: the headless `App` plus the
//! shell's own bookkeeping.

use crate::paths::{AppDirs, Profile};
use riddle_app::{App, AppConfig};
use riddle_core::BuildStamp;
use riddle_store::Stores;
use std::sync::Arc;
use std::time::Instant;

pub struct AppState {
    pub app: Arc<App>,
    pub handle: tauri::AppHandle,
    pub dirs: AppDirs,
    pub profile: Profile,
    pub build: BuildStamp,
    pub started_at: Instant,
}

impl AppState {
    pub fn bootstrap(
        handle: tauri::AppHandle,
        dirs: AppDirs,
        profile: Profile,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let build = BuildStamp::from_env_vars(
            env!("CARGO_PKG_VERSION"),
            option_env!("RIDDLE_GIT_SHA"),
            option_env!("RIDDLE_GIT_DIRTY"),
            option_env!("RIDDLE_BUILT_AT"),
            profile.as_str(),
        );
        let app = App::open(AppConfig {
            data_dir: Some(dirs.data.clone()),
            config_dir: Some(dirs.config.clone()),
            dev: profile.is_dev(),
            build: build.clone(),
            keychain_service: profile.keychain_service(),
            offline: false,
            log_dir: Some(dirs.logs.clone()),
        })?;
        Ok(Self {
            app,
            handle,
            dirs,
            profile,
            build,
            started_at: Instant::now(),
        })
    }

    pub fn stores(&self) -> Arc<Stores> {
        self.app.stores()
    }

    /// Close-to-hide from settings; on until settings are loaded.
    pub fn close_to_hide(&self) -> bool {
        self.app
            .services
            .get::<riddle_settings::SettingsStore>()
            .map(|s| s.settings().close_to_hide)
            .unwrap_or(true)
    }
    pub fn runtime(&self) -> Arc<riddle_actions::Runtime> {
        self.app.runtime.clone()
    }
}
