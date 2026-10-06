//! Process presence (ARCHITECTURE.md §5): a tray item with Open, Refresh
//! now, Reset window position and Quit; close hides the window; Dock
//! reopen restores it; a focus or visibility change triggers a refresh pass.
//!
//! The window's size and place are saved when they settle, not only at quit.
//! The window-state plugin writes its file on a clean exit alone, and Riddle
//! rarely has one: close hides the window, and `tauri dev` and system
//! restarts end the process outright. The file went unwritten for a week and
//! every launch opened at the size it held then, nearly the whole screen.

use crate::state::AppState;
use riddle_core::{ActionInvocation, Principal};
use riddle_observability::{rdebug, rinfo, Category};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// What the window remembers between runs: everything but whether it was
/// shown, since it opens shown and a hidden-at-quit window would stay lost.
pub fn window_state_flags() -> StateFlags {
    StateFlags::all() - StateFlags::VISIBLE
}

/// Bumped by every move or resize; a save runs only if none came after it.
static WINDOW_CHANGES: AtomicU64 = AtomicU64::new(0);

/// Save the window's size and place once it has stopped moving for half a
/// second; a drag sends dozens of events and needs one write.
fn save_window_state_soon(app: &AppHandle) {
    let seen = WINDOW_CHANGES.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if WINDOW_CHANGES.load(Ordering::Relaxed) == seen {
            save_window_state(&app);
        }
    });
}

fn save_window_state(app: &AppHandle) {
    if let Err(e) = app.save_window_state(window_state_flags()) {
        rdebug!(Category::App, "window state not saved: {e}");
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Riddle", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "Reset window position", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Riddle", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh, &reset, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .icon_as_template(false)
        .tooltip("Riddle")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "refresh" => refresh_now(app.clone()),
            "reset" => reset_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Off-screen window rescue after monitor changes.
pub fn reset_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
            80.0, 80.0,
        )));
        let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(1440.0, 900.0)));
        let _ = w.show();
        let _ = w.set_focus();
        rinfo!(Category::App, "window position reset");
    }
}

/// A refresh pass through the Action layer; harmless before the sync track
/// registers `refresh_now`.
pub fn refresh_now(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let inv = ActionInvocation {
            action: "refresh_now".into(),
            args: serde_json::json!({}),
            principal: Principal::System,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        };
        match state.runtime().invoke(inv).await {
            Ok(_) => rdebug!(Category::App, "refresh requested"),
            Err(e) => rdebug!(Category::App, "refresh not available: {}", e.kind.as_str()),
        }
    });
}

/// Window event wiring: close-to-hide and focus-triggered refresh.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            let close_to_hide = window
                .app_handle()
                .try_state::<AppState>()
                .map(|s| s.close_to_hide())
                .unwrap_or(true);
            if close_to_hide {
                api.prevent_close();
                save_window_state(window.app_handle());
                let _ = window.hide();
                rdebug!(Category::App, "window hidden; process stays resident");
            }
        }
        WindowEvent::Focused(true) => refresh_now(window.app_handle().clone()),
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            save_window_state_soon(window.app_handle())
        }
        _ => {}
    }
}
