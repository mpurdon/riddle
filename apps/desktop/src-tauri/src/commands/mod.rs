//! Command registration. Each module is one feature area; every command is a
//! thin adapter returning `Result<T, RiddleError>`.

pub mod dev;

pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        dev::dev_info,
        dev::ui_log,
        dev::read_app_logs,
        dev::log_categories,
        dev::in_flight_calls,
    ]
}
