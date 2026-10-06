//! Bridges the app's broadcast events to one Tauri event, `riddle://event`.
//! Events carry signals; the webview refetches what it needs (§7).

use riddle_core::RiddleEvent;
use riddle_observability::{rdebug, Category};
use tauri::Emitter;

pub fn spawn_bridge(handle: tauri::AppHandle, app: std::sync::Arc<riddle_app::App>) {
    let mut rx = app.events.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let name = event_name(&event);
                    if let Err(e) = handle.emit(RiddleEvent::NAME, &event) {
                        rdebug!(Category::Ui, "event {name} not delivered: {e}");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    rdebug!(Category::Ui, "event bridge lagged by {n}");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn event_name(e: &RiddleEvent) -> &'static str {
    match e {
        RiddleEvent::JobChanged { .. } => "job_changed",
        RiddleEvent::JobProgress { .. } => "job_progress",
        RiddleEvent::DigStatusChanged { .. } => "dig_status",
        RiddleEvent::FindCreated { .. } => "find_created",
        RiddleEvent::FindChanged { .. } => "find_changed",
        RiddleEvent::ActivityAppended { .. } => "activity",
        RiddleEvent::MessageClassified { .. } => "classified",
        RiddleEvent::MessagesChanged { .. } => "messages",
        RiddleEvent::SummaryChanged => "summary",
        RiddleEvent::StoreFault { .. } => "store_fault",
        RiddleEvent::StoreFaultCleared => "store_fault_cleared",
        RiddleEvent::Notify { .. } => "notify",
        RiddleEvent::GmailSignIn { .. } => "gmail_sign_in",
        RiddleEvent::UpdateAvailable { .. } => "update",
        RiddleEvent::PipelineStage { .. } => "pipeline",
        RiddleEvent::DirectorDelta { .. } => "director_delta",
        RiddleEvent::DirectorToolCall { .. } => "director_tool",
        RiddleEvent::DirectorDone { .. } => "director_done",
        RiddleEvent::ProposalCreated { .. } => "proposal",
        RiddleEvent::ExpeditionChanged { .. } => "expedition",
        RiddleEvent::SettingsChanged => "settings",
    }
}
