//! The Tauri adapter. Thin by design (ARCHITECTURE.md §7): commands
//! deserialize, call an Action or a read service, and serialize. Everything
//! that matters lives in the `riddle-*` crates.

pub mod commands;
pub mod events;
pub mod logging;
pub mod paths;
pub mod presence;
pub mod state;
pub mod system;

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
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(presence::window_state_flags())
                .build(),
        )
        .setup(move |app| {
            log::set_max_level(logging::initial_level(profile));
            riddle_observability::install_panic_hook();
            let state = state::AppState::bootstrap(app.handle().clone(), dirs.clone(), profile)?;
            let stamp = state.build.clone();
            events::spawn_bridge(app.handle().clone(), state.app.clone());
            {
                // Boot the core (settings, domains, AI, judge, jobs); the judge
                // may resolve its key from SSM, so this runs off the main thread.
                let core = state.app.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match core.boot(true).await {
                        Ok(ops) => {
                            rinfo!(
                                Category::App,
                                "core booted; {} actions",
                                core.runtime.list_actions().len()
                            );
                            system::start(handle, core.clone(), ops.settings.clone());
                        }
                        Err(e) => riddle_observability::rerror!(
                            Category::App,
                            "core boot failed: {}: {}",
                            e.kind.as_str(),
                            e.message
                        ),
                    }
                });
            }
            if profile.is_dev() {
                // Dev builds expose the loopback bridge for automation (§59, §91).
                let core = state.app.clone();
                let token_file = dirs.data.join("bridge.json");
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = riddle_app::bridge::start(core, 1431, Some(token_file)).await {
                        riddle_observability::rwarn!(
                            Category::App,
                            "dev bridge did not start: {e}"
                        );
                    }
                });
            }
            app.manage(state);
            rinfo!(
                Category::App,
                "riddle {} ({}{}) started; profile={:?}; data={}",
                stamp.version,
                stamp.git_sha,
                if stamp.dirty { "-dirty" } else { "" },
                profile,
                dirs.data.display()
            );
            if let Some(w) = app.get_webview_window("main") {
                let title = match profile {
                    paths::Profile::Dev => format!("Riddle DEV {}", stamp.git_sha),
                    paths::Profile::Prod => "Riddle".to_string(),
                };
                let _ = w.set_title(&title);
            }
            presence::install(app.handle())?;
            Ok(())
        })
        .on_window_event(presence::on_window_event)
        .invoke_handler(commands::handler())
        .build(tauri::generate_context!())
        .expect("error while building Riddle")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                presence::show_main(app);
            }
            let _ = (&app, &event);
        });
}
