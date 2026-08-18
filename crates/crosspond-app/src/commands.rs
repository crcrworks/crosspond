use std::path::{Path, PathBuf};
use std::process::Command;

use crosspond_core::{
    AppConfig, ApprovalId, ComputerApprovalMode, MISSING_API_KEY_MESSAGE, Receipt, RuntimeCommand,
    SecretKey, SecretString, StartTaskRequest, TaskId, default_tasks_root, history_group_label,
    history_title, list_recent_tasks, provider_key_is_set,
};
use crosspond_macos::{PermissionKind, PermissionSnapshot};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use crate::launcher;
use crate::state::AppState;

#[derive(Serialize)]
pub struct Bootstrap {
    pub needs_onboarding: bool,
    pub computer_approval: ComputerApprovalMode,
    pub badges: Vec<String>,
    pub visible: bool,
}

#[derive(Serialize)]
pub struct SettingsView {
    pub base_url: String,
    pub model: String,
    pub provider_key_stored: bool,
    pub exa_key_stored: bool,
    pub permissions: PermissionSnapshot,
    pub computer_approval: ComputerApprovalMode,
}

#[derive(Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub status_mark: String,
    pub group: String,
    pub receipt: Option<Receipt>,
    pub artifact_names: Vec<String>,
}

#[tauri::command]
pub fn bootstrap(state: State<AppState>) -> Bootstrap {
    let inner = state.lock_inner();
    Bootstrap {
        needs_onboarding: !provider_key_is_set(&*state.secrets),
        computer_approval: state
            .config
            .load()
            .map(|config| config.computer_approval)
            .unwrap_or_default(),
        badges: inner.ambient.badge_lines(),
        visible: inner.visible,
    }
}

#[tauri::command]
pub fn start_task(
    prompt: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<String, String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("prompt is empty".into());
    }
    if !provider_key_is_set(&*state.secrets) {
        return Err(MISSING_API_KEY_MESSAGE.into());
    }
    let task_id = TaskId::new();
    let mut inner = state.lock_inner();
    let context = inner.ambient.clone();
    inner.current_task = Some(task_id);
    inner.in_conversation = true;
    inner.compact = false;
    inner.artifacts.clear();
    let seq = inner.bump_resize_seq();
    drop(inner);
    // Expand here so New's in-flight compact resize cannot land after send.
    launcher::request_resize_with_seq(&app, false, 0, 0.0, seq);
    state
        .commands
        .send(RuntimeCommand::StartTask(StartTaskRequest {
            task_id,
            prompt,
            context,
        }));
    Ok(task_id.to_string())
}

#[tauri::command]
pub fn approve(id: ApprovalId, state: State<AppState>) {
    state.commands.send(RuntimeCommand::Approve(id));
}

#[tauri::command]
pub fn reject(id: ApprovalId, state: State<AppState>) {
    state.commands.send(RuntimeCommand::Reject(id));
}

#[tauri::command]
pub fn cancel(state: State<AppState>) {
    if let Some(task_id) = state.lock_inner().current_task {
        state.commands.send(RuntimeCommand::Cancel(task_id));
    }
}

#[tauri::command]
pub fn reset_session(app: AppHandle, state: State<AppState>) {
    if let Some(task_id) = state.lock_inner().current_task {
        state.commands.send(RuntimeCommand::Cancel(task_id));
    }
    state.commands.send(RuntimeCommand::ResetSession);
    {
        let mut inner = state.lock_inner();
        inner.current_task = None;
        inner.in_conversation = false;
        inner.compact = true;
        inner.artifacts.clear();
        inner.ambient = Default::default();
    }
    launcher::recollect_ambient(&app);
}

#[tauri::command]
pub fn hide_launcher(app: AppHandle) {
    launcher::hide(&app);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("/settings".into()))
        .title("Settings")
        .inner_size(480.0, 640.0)
        .min_inner_size(400.0, 480.0)
        .resizable(true)
        .build()
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn load_settings(state: State<AppState>) -> SettingsView {
    let loaded = state.config.load().unwrap_or_default();
    let provider_key_stored = provider_key_is_set(&*state.secrets);
    let exa_key_stored = state
        .secrets
        .get(&SecretKey::EXA_API_KEY)
        .ok()
        .flatten()
        .is_some_and(|key| !key.is_empty());
    SettingsView {
        base_url: loaded.base_url,
        model: loaded.model,
        provider_key_stored,
        exa_key_stored,
        permissions: PermissionSnapshot::current(),
        computer_approval: loaded.computer_approval,
    }
}

#[tauri::command]
pub fn save_config(base_url: String, model: String, state: State<AppState>) -> Result<(), String> {
    let mut config = state.config.load().unwrap_or_default();
    let defaults = AppConfig::default();
    config.base_url = if base_url.trim().is_empty() {
        defaults.base_url
    } else {
        base_url.trim().to_string()
    };
    config.model = if model.trim().is_empty() {
        defaults.model
    } else {
        model.trim().to_string()
    };
    state.config.save(&config).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_secret(kind: String, value: String, state: State<AppState>) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    let key = match kind.as_str() {
        "provider" => SecretKey::PROVIDER_API_KEY,
        "exa" => SecretKey::EXA_API_KEY,
        _ => return Err("unknown secret".into()),
    };
    state
        .secrets
        .set(&key, &SecretString::new(value))
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn test_connection(state: State<AppState>) {
    state.commands.send(RuntimeCommand::TestConnection);
}

#[tauri::command]
pub fn list_history() -> Vec<HistoryItem> {
    let now = std::time::SystemTime::now();
    list_recent_tasks(&default_tasks_root(), 50)
        .into_iter()
        .map(|entry| HistoryItem {
            title: history_title(&entry.prompt),
            status_mark: entry.status_mark().to_string(),
            group: history_group_label(entry.modified, now).to_string(),
            artifact_names: entry
                .receipt
                .as_ref()
                .map(|receipt| receipt.artifacts.clone())
                .unwrap_or_default(),
            id: entry.id,
            status: entry.status,
            receipt: entry.receipt,
        })
        .collect()
}

#[tauri::command]
pub fn cycle_computer_approval(state: State<AppState>) -> Result<ComputerApprovalMode, String> {
    let mut config = state.config.load().unwrap_or_default();
    config.computer_approval = config.computer_approval.cycle();
    state.config.save(&config).map_err(|err| err.to_string())?;
    Ok(config.computer_approval)
}

#[tauri::command]
pub fn permissions() -> PermissionSnapshot {
    PermissionSnapshot::current()
}

#[tauri::command]
pub fn open_system_settings(kind: String, app: AppHandle) -> Result<(), String> {
    let kind = match kind.as_str() {
        "accessibility" => PermissionKind::Accessibility,
        "screen_recording" => PermissionKind::ScreenRecording,
        "calendars" => PermissionKind::Calendars,
        _ => return Err("unknown permission".into()),
    };
    app.opener()
        .open_url(kind.settings_url(), None::<&str>)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn reveal_artifact(name: String, state: State<AppState>) -> Result<(), String> {
    let inner = state.lock_inner();
    let path = inner
        .artifacts
        .iter()
        .find(|(stored, _)| stored == &name)
        .map(|(_, path)| path.clone())
        .ok_or_else(|| "artifact not found".to_string())?;
    drop(inner);
    reveal_in_finder(&path)
}

#[tauri::command]
pub fn reveal_history_artifact(task_id: String, name: String) -> Result<(), String> {
    let entries = list_recent_tasks(&default_tasks_root(), 50);
    let entry = entries
        .iter()
        .find(|entry| entry.id == task_id)
        .ok_or_else(|| "task not found".to_string())?;
    let path = entry
        .artifact_path(&name)
        .ok_or_else(|| "artifact not found".to_string())?;
    reveal_in_finder(&path)
}

#[tauri::command]
pub fn set_ui_flags(compact: bool, composing: bool, in_conversation: bool, state: State<AppState>) {
    let mut inner = state.lock_inner();
    inner.compact = compact;
    inner.composing = composing;
    inner.in_conversation = in_conversation;
}

#[tauri::command]
pub fn sync_launcher_size(compact: bool, badge_lines: u32, extra_height: f64, app: AppHandle) {
    launcher::request_resize(&app, compact, badge_lines as usize, extra_height);
}

fn reveal_in_finder(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn store_artifact(state: &AppState, name: String, path: PathBuf) {
    state.lock_inner().artifacts.push((name, path));
}
