//! Developer pane commands: runtime, storage, logs, in-flight calls (§67).

use crate::state::AppState;
use riddle_core::BuildStamp;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{record, tail, Category, LogLine, SwallowedFailure};
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
    /// Bookkeeping failures logged and dropped since launch, by site.
    pub swallowed: Vec<SwallowedFailure>,
}

#[tauri::command]
pub async fn dev_info(state: State<'_, AppState>) -> Result<DevInfo> {
    let stores_ref = state.stores();
    let mut stores = vec![StoreInfo {
        label: "index".into(),
        path: stores_ref
            .index()
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        size_bytes: stores_ref.index().size_bytes(),
    }];
    for (id, s) in stores_ref.open_digs() {
        stores.push(StoreInfo {
            label: format!("dig {}", id),
            path: s
                .path()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            size_bytes: s.size_bytes(),
        });
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
        store_fault: stores_ref.any_fault(),
        log_level: log::max_level().to_string(),
        swallowed: riddle_observability::swallowed_failures(),
    })
}

/// Webview log lines land in the backend log and the tail. Never fails.
#[tauri::command]
pub async fn ui_log(level: String, category: String, message: String) {
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
pub async fn read_app_logs(
    limit: Option<u32>,
    category: Option<String>,
    level: Option<String>,
    text: Option<String>,
) -> Result<Vec<LogLine>> {
    let min = level.as_deref().and_then(|l| l.parse::<log::Level>().ok());
    Ok(tail(
        limit.unwrap_or(500) as usize,
        category.as_deref(),
        min,
        text.as_deref(),
    ))
}

#[tauri::command]
pub async fn log_categories() -> Vec<&'static str> {
    Category::ALL.iter().map(|c| c.as_str()).collect()
}

#[allow(dead_code)]
fn _assert_error_is_serializable(e: RiddleError) -> String {
    serde_json::to_string(&e).unwrap_or_default()
}

/// The redacted diagnostics bundle, from the shared router so the desktop,
/// the bridge and the CLI export the same document; the desktop's own
/// directories ride along, which the router does not know. A JSON object:
/// the pane pretty-prints it for the clipboard.
#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> Result<serde_json::Value> {
    let mut bundle =
        riddle_app::dispatch::call(&state.app, "export_diagnostics", serde_json::json!({})).await?;
    if let Some(runtime) = bundle.get_mut("runtime").and_then(|r| r.as_object_mut()) {
        runtime.insert(
            "logDir".into(),
            serde_json::json!(state.dirs.logs.display().to_string()),
        );
        runtime.insert(
            "cacheDir".into(),
            serde_json::json!(state.dirs.cache.display().to_string()),
        );
    }
    Ok(bundle)
}

/// Drop derived caches (the in-memory log tail keeps its lines; SQLite
/// caches are compacted). Stores are never touched.
#[tauri::command]
pub async fn clear_caches(state: State<'_, AppState>) -> Result<u64> {
    let mut freed = 0u64;
    if state.dirs.cache.exists() {
        if let Ok(rd) = std::fs::read_dir(&state.dirs.cache) {
            for entry in rd.flatten() {
                if let Ok(m) = entry.metadata() {
                    freed += m.len();
                }
                let _ = if entry.path().is_dir() {
                    std::fs::remove_dir_all(entry.path())
                } else {
                    std::fs::remove_file(entry.path())
                };
            }
        }
    }
    let _ = state.stores().index().compact();
    Ok(freed)
}
