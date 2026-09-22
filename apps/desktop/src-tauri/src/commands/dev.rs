//! Developer pane commands: runtime, storage, logs, in-flight calls (§67).

use crate::state::AppState;
use riddle_core::BuildStamp;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{record, tail, Category, LogLine};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreInfo {
    pub label: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevInfo {
    pub build: BuildStamp,
    pub dev: bool,
    pub os: String,
    pub arch: String,
    pub uptime_seconds: u64,
    pub data_dir: String,
    pub log_dir: String,
    pub config_dir: String,
    pub cache_dir: String,
    pub stores: Vec<StoreInfo>,
    pub store_fault: Option<String>,
    pub log_level: String,
}

#[tauri::command]
pub fn dev_info(state: State<'_, AppState>) -> Result<DevInfo> {
    let mut stores = vec![StoreInfo {
        label: "index".into(),
        path: state.stores.index().path().map(|p| p.display().to_string()).unwrap_or_default(),
        size_bytes: state.stores.index().size_bytes(),
    }];
    for (id, s) in state.stores.open_digs() {
        stores.push(StoreInfo { label: format!("dig {}", id), path: s.path().map(|p| p.display().to_string()).unwrap_or_default(), size_bytes: s.size_bytes() });
    }
    Ok(DevInfo {
        build: state.build.clone(),
        dev: state.profile.is_dev(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        uptime_seconds: state.started_at.elapsed().as_secs(),
        data_dir: state.dirs.data.display().to_string(),
        log_dir: state.dirs.logs.display().to_string(),
        config_dir: state.dirs.config.display().to_string(),
        cache_dir: state.dirs.cache.display().to_string(),
        stores,
        store_fault: state.stores.any_fault(),
        log_level: log::max_level().to_string(),
    })
}

/// Webview log lines land in the backend log and the tail. Never fails.
#[tauri::command]
pub fn ui_log(level: String, category: String, message: String) {
    let lvl = match level.as_str() {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        "info" => log::Level::Info,
        "debug" => log::Level::Debug,
        _ => log::Level::Trace,
    };
    let cat = Category::parse(&category).unwrap_or(Category::Ui);
    record(lvl, cat.as_str(), &message);
    log::log!(target: cat.as_str(), lvl, "{message}");
}

#[tauri::command]
pub fn read_app_logs(limit: Option<u32>, category: Option<String>, level: Option<String>, text: Option<String>) -> Result<Vec<LogLine>> {
    let min = level.as_deref().and_then(|l| l.parse::<log::Level>().ok());
    Ok(tail(limit.unwrap_or(500) as usize, category.as_deref(), min, text.as_deref()))
}

#[tauri::command]
pub fn log_categories() -> Vec<&'static str> {
    Category::ALL.iter().map(|c| c.as_str()).collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InFlight {
    pub command: String,
    pub elapsed_ms: u64,
}

#[tauri::command]
pub fn in_flight_calls(state: State<'_, AppState>) -> Result<Vec<InFlight>> {
    let g = state.in_flight.lock();
    let mut v: Vec<InFlight> = g.values().map(|(c, t)| InFlight { command: c.clone(), elapsed_ms: t.elapsed().as_millis() as u64 }).collect();
    v.sort_by(|a, b| b.elapsed_ms.cmp(&a.elapsed_ms));
    Ok(v)
}

#[allow(dead_code)]
fn _assert_error_is_serializable(e: RiddleError) -> String { serde_json::to_string(&e).unwrap_or_default() }
