//! Command registration. Each module is one feature area; every command is a
//! thin adapter returning `Result<T, RiddleError>`.

pub mod actions;
pub mod dev;
pub mod more;
pub mod reads;

pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        dev::dev_info,
        dev::ui_log,
        dev::read_app_logs,
        dev::log_categories,
        dev::in_flight_calls,
        dev::export_diagnostics,
        dev::clear_caches,
        actions::invoke_action,
        actions::list_actions,
        actions::list_pending,
        actions::dev_armed,
        reads::summary,
        reads::list_digs,
        reads::list_dig_groups,
        reads::list_messages,
        reads::get_message,
        reads::get_thread,
        reads::explain,
        reads::search,
        reads::list_finds,
        reads::get_find,
        reads::list_activity,
        reads::get_activity,
        reads::list_meshes,
        reads::get_mesh,
        reads::mesh_versions,
        reads::list_signals,
        reads::signal_versions,
        reads::signal_evaluations,
        reads::list_strata,
        reads::list_expeditions,
        reads::get_expedition,
        reads::list_drafts,
        reads::get_user_profile,
        reads::read_trace,
        reads::recent_traces,
        reads::usage_summary,
        reads::trace_stats,
        more::task_centre,
        more::get_settings,
        more::privacy_statement,
        more::probe_judge,
        more::probe_llm,
        more::probe_provider,
        more::update_preview,
        more::director_send,
        more::list_conversations,
        more::get_conversation,
    ]
}
