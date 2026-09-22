//! The headless application: stores, services, the Action runtime and the
//! read services every client shares (desktop, CLI, MCP). "One Riddle"
//! (ARCHITECTURE.md §3) is this crate: the UI and the Director are two
//! clients of the same `App`.
//!
//! `assemble` wires the domain crates in; `reads` are the query services
//! commands call; `events` is the broadcast sink clients subscribe to.

pub mod ai;
pub mod assemble;
pub mod bridge;
pub mod director_reads;
pub mod dispatch;
pub mod paths;
pub mod events;
pub mod reads;

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
}

impl AppConfig {
    pub fn in_memory(build: BuildStamp) -> Self {
        Self { data_dir: None, config_dir: None, dev: true, build, keychain_service: "dev.codenaked.riddle.test".into() }
    }
}

pub struct App {
    pub config: AppConfig,
    pub stores: Arc<Stores>,
    pub services: Arc<Services>,
    pub runtime: Arc<Runtime>,
    pub events: Arc<events::Broadcast>,
}

impl App {
    pub fn stores(&self) -> Arc<Stores> { self.stores.clone() }
}
