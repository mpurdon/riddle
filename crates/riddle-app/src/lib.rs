//! The headless application: stores, services, the Action runtime and the
//! read services every client shares (desktop, CLI, MCP). "One Riddle"
//! (ARCHITECTURE.md §3) is this crate: the UI and the Director are two
//! clients of the same `App`.
//!
//! `assemble` wires the domain crates in; `reads` are the query services
//! commands call; `events` is the broadcast sink clients subscribe to.

pub mod actions;
pub mod ai;
pub mod assemble;
pub mod bridge;
pub mod comparison;
pub mod director_reads;
pub mod dispatch;
pub mod domains;
pub mod engine;
pub mod events;
pub mod facts_refresh;
pub mod live;
pub mod notify;
pub mod ops;
pub mod packs;
pub mod paths;
pub mod places;
pub mod preview;
pub mod reads;
pub mod suggestions;
pub mod window;
pub mod words;

use riddle_actions::{Runtime, Services};
use riddle_core::BuildStamp;
use riddle_store::Stores;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub dev: bool,
    pub build: BuildStamp,
    pub keychain_service: String,
    /// No network and no keychain for this run: the scripted judge and no
    /// LLM are applied in memory at boot, and credentials live in memory.
    /// The CLI's `--offline` sets it, for rule work on the local store and
    /// for scripts: macOS asks for permission the first time a freshly built
    /// binary reads a keychain item, and a headless command that reads one
    /// at boot hangs on that dialog.
    pub offline: bool,
    /// Where the log file lives (`AppDirs::logs`), so the Logs pane can
    /// tail it after a restart and `dev_info` can name it; `None` for an
    /// in-memory app, which logs to its ring only.
    pub log_dir: Option<PathBuf>,
}

impl AppConfig {
    pub fn in_memory(build: BuildStamp) -> Self {
        Self {
            data_dir: None,
            config_dir: None,
            dev: true,
            build,
            keychain_service: "dev.codenaked.riddle.test".into(),
            offline: false,
            log_dir: None,
        }
    }
}

pub struct App {
    pub config: AppConfig,
    pub started_at: std::time::Instant,
    pub stores: Arc<Stores>,
    pub services: Arc<Services>,
    pub runtime: Arc<Runtime>,
    pub events: Arc<events::Broadcast>,
}

impl App {
    pub fn stores(&self) -> Arc<Stores> {
        self.stores.clone()
    }
}
