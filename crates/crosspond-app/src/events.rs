use crosspond_core::{AgentEvent, EventPump};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::store_artifact;
use crate::state::AppState;

pub fn start_event_loop(app: AppHandle, mut events: EventPump) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let AgentEvent::ArtifactCreated {
                display_name, path, ..
            } = &event
                && let Some(state) = app.try_state::<AppState>()
            {
                store_artifact(&state, display_name.clone(), path.clone());
            }
            let _ = app.emit("agent-event", &event);
        }
    });
}
