//! The settings that belong to the operating system rather than the core:
//! log level, launch at login, system notifications and the dock badge.
//!
//! All three used to be saved and never applied. The log level waited for
//! an env var that the Settings page never set, launch at login had no
//! plugin, and nothing sent a notification. `start` applies them once the
//! core has booted and again on every save. `RIDDLE_LOG`, when set, still
//! overrides the saved log level.
//!
//! Which Finds interrupt and what the brief says is policy and lives in
//! `riddle_app::notify`; this module only schedules the brief, shows what it
//! is handed, and keeps the dock badge, which counts decisions waiting (open
//! Finds and Unresolved), never messages.
//!
//! On macOS 26 the notification plugin reports success and shows nothing
//! (ARCHITECTURE.md, Notifications), so macOS notifications go through `osascript`.
//! The title and body are passed as script arguments, never spliced into
//! the script text.

use riddle_app::notify::notification_for;
use riddle_core::RiddleEvent;
use riddle_observability::{rinfo, rwarn, Category};
use riddle_settings::{Settings, SettingsStore};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Apply the OS-level settings now and whenever they change, route Finds to
/// notifications by the policy above, send the briefs, and keep the badge.
pub fn start(handle: AppHandle, app: Arc<riddle_app::App>, store: Arc<SettingsStore>) {
    let mut rx = store.subscribe();
    let h = handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut prev: Option<Settings> = None;
        loop {
            let next = rx.borrow_and_update().clone();
            apply(&h, prev.as_ref(), &next);
            prev = Some(next);
            if rx.changed().await.is_err() {
                break;
            }
        }
    });

    let mut events = app.events.subscribe();
    let (badge_tx, mut badge_rx) = tokio::sync::mpsc::channel::<()>(4);
    {
        let handle = handle.clone();
        let app = app.clone();
        let store = store.clone();
        tauri::async_runtime::spawn(async move {
            let _ = badge_tx.try_send(());
            loop {
                match events.recv().await {
                    Ok(RiddleEvent::FindCreated { find }) => {
                        let settings = store.settings();
                        if let Some((title, body)) = notification_for(&app, &settings, &find) {
                            notify(&handle, &title, &body);
                        }
                        let _ = badge_tx.try_send(());
                    }
                    Ok(RiddleEvent::FindChanged { .. })
                    | Ok(RiddleEvent::SummaryChanged)
                    | Ok(RiddleEvent::MessageClassified { .. })
                    | Ok(RiddleEvent::MessagesChanged { .. }) => {
                        let _ = badge_tx.try_send(());
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // The badge: recomputed at most every two seconds, from the Summary.
    {
        let handle = handle.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while badge_rx.recv().await.is_some() {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                while badge_rx.try_recv().is_ok() {}
                let count = riddle_app::notify::decisions_waiting(&app).unwrap_or(0);
                set_badge(&handle, count);
            }
        });
    }

    // The brief: the policy decides whether one is due this minute.
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let settings = store.settings();
            match riddle_app::notify::brief_due(&app, &settings, chrono::Local::now()) {
                Ok(Some(body)) => notify(&handle, "Riddle brief", &body),
                Ok(None) => {}
                Err(e) => rwarn!(Category::Ui, "brief skipped: {}", e.message),
            }
        }
    });
}

fn set_badge(handle: &AppHandle, count: u64) {
    if let Some(w) = handle.get_webview_window("main") {
        let value = if count > 0 { Some(count as i64) } else { None };
        if let Err(e) = w.set_badge_count(value) {
            rwarn!(Category::Ui, "badge not set: {e}");
        }
    }
}

fn apply(handle: &AppHandle, prev: Option<&Settings>, next: &Settings) {
    if prev.map(|p| p.log_level) != Some(next.log_level) && std::env::var("RIDDLE_LOG").is_err() {
        log::set_max_level(next.log_level.to_level_filter());
        rinfo!(Category::App, "log level {}", next.log_level.as_str());
    }
    if prev.map(|p| p.launch_at_login) != Some(next.launch_at_login) {
        set_launch_at_login(handle, next.launch_at_login);
    }
}

fn set_launch_at_login(handle: &AppHandle, on: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = handle.autolaunch();
    let current = launcher.is_enabled().unwrap_or(false);
    if current == on {
        return;
    }
    let result = if on {
        launcher.enable()
    } else {
        launcher.disable()
    };
    match result {
        Ok(()) => rinfo!(
            Category::App,
            "launch at login {}",
            if on { "on" } else { "off" }
        ),
        Err(e) => rwarn!(Category::App, "launch at login not changed: {e}"),
    }
}

/// Show a system notification. Failure is logged and swallowed: a missed
/// notification never fails the work that produced it.
pub fn notify(handle: &AppHandle, title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = handle;
        let (title, body) = (title.to_string(), body.to_string());
        // osascript takes a moment; keep it off the async runtime.
        std::thread::spawn(move || {
            let out = std::process::Command::new("osascript")
                .args([
                    "-e",
                    "on run argv",
                    "-e",
                    "display notification (item 2 of argv) with title (item 1 of argv)",
                    "-e",
                    "end run",
                    &title,
                    &body,
                ])
                .output();
            match out {
                Ok(o) if o.status.success() => {}
                Ok(o) => rwarn!(
                    Category::Ui,
                    "notification not shown: osascript exit {}",
                    o.status
                ),
                Err(e) => rwarn!(Category::Ui, "notification not shown: {e}"),
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        if let Err(e) = handle
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            rwarn!(Category::Ui, "notification not shown: {e}");
        }
    }
}
