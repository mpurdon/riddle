//! The Tauri adapter. Thin by design (ARCHITECTURE.md §7): commands
//! deserialize, call an Action or a read service, and serialize. Everything
//! that matters lives in the `riddle-*` crates.

pub mod commands;
pub mod events;
pub mod logging;
pub mod paths;
pub mod state;

use riddle_observability::{rinfo, Category};
use tauri::Manager;

pub fn run() {
    let profile = paths::Profile::current();
    let dirs = paths::AppDirs::resolve(profile);
    if let Err(e) = std::fs::create_dir_all(&dirs.data) {
        eprintln!("cannot create data dir {}: {e}", dirs.data.display());
    }
    let log_plugin = logging::plugin(&dirs, profile);

    tauri::Builder::default()
        .plugin(log_plugin)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE)
                .build(),
        )
        .setup(move |app| {
            let state = state::AppState::bootstrap(app.handle().clone(), dirs.clone(), profile)?;
            let stamp = state.build.clone();
            events::spawn_bridge(app.handle().clone(), state.app.clone());
            app.manage(state);
            rinfo!(Category::App, "riddle {} ({}{}) started; profile={:?}; data={}",
                stamp.version, stamp.git_sha, if stamp.dirty { "-dirty" } else { "" }, profile, dirs.data.display());
            if let Some(w) = app.get_webview_window("main") {
                let title = match profile {
                    paths::Profile::Dev => format!("Riddle DEV {}", stamp.git_sha),
                    paths::Profile::Prod => "Riddle".to_string(),
                };
                let _ = w.set_title(&title);
            }
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("error while running Riddle");
}
