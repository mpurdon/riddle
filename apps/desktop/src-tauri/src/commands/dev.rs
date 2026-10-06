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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InFlight {
    pub command: String,
    pub elapsed_ms: u64,
}

#[tauri::command]
pub async fn in_flight_calls(state: State<'_, AppState>) -> Result<Vec<InFlight>> {
    let g = state.in_flight.lock();
    let mut v: Vec<InFlight> = g
        .values()
        .map(|(c, t)| InFlight {
            command: c.clone(),
            elapsed_ms: t.elapsed().as_millis() as u64,
        })
        .collect();
    v.sort_by_key(|i| std::cmp::Reverse(i.elapsed_ms));
    Ok(v)
}

#[allow(dead_code)]
fn _assert_error_is_serializable(e: RiddleError) -> String {
    serde_json::to_string(&e).unwrap_or_default()
}

/// A redacted diagnostics block safe to paste into an issue: build, OS,
/// directories, store sizes, fault state, and the last warnings. No
/// identifiers that could carry content, no settings values that are secrets.
#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> Result<String> {
    let info = dev_info(state.clone()).await?;
    let mut out = String::new();
    out.push_str(&format!(
        "Riddle {} ({}{}) built {} [{}]\n",
        info.build.version,
        info.build.git_sha,
        if info.build.dirty { "-dirty" } else { "" },
        info.build.built_at,
        info.build.profile
    ));
    out.push_str(&format!(
        "OS {} {}; uptime {}s; log level {}\n",
        info.os, info.arch, info.uptime_seconds, info.log_level
    ));
    out.push_str(&format!("data {}\nlogs {}\n", info.data_dir, info.log_dir));
    for s in &info.stores {
        out.push_str(&format!("store {} {} KB\n", s.label, s.size_bytes / 1024));
    }
    if let Some(f) = &info.store_fault {
        out.push_str(&format!("STORE FAULT: {f}\n"));
    }
    match state.app.list_digs() {
        Ok(digs) => out.push_str(&format!(
            "digs {} ({})\n",
            digs.len(),
            digs.iter()
                .map(|d| format!("{:?}/{:?}", d.provider, d.status))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Err(e) => out.push_str(&format!("digs unavailable: {}\n", e.kind.as_str())),
    }
    out.push_str(&format!(
        "actions {}\n",
        state.runtime().list_actions().len()
    ));
    out.push_str("recent warnings:\n");
    for l in tail(50, None, Some(log::Level::Warn), None) {
        out.push_str(&format!(
            "  [{}][{}][{}] {}\n",
            l.at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            l.level,
            l.category,
            l.message
        ));
    }
    Ok(out)
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
