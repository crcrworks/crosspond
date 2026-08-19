use std::path::{Path, PathBuf};
use std::process::Command;

use crosspond_core::{
    AppConfig, ApprovalId, ComputerApprovalMode, ConversationId, ConversationView, HotkeyView,
    LauncherHotkey, MISSING_API_KEY_MESSAGE, Mention, Receipt, RuntimeCommand, SecretKey,
    SecretString, StartTaskRequest, TaskId, conversation_artifact_path, default_tasks_root,
    default_vault_path, history_group_label, history_title, list_recent_tasks,
    open_conversation as load_conversation, parse_vault_path_input, provider_key_is_set,
};
use crosspond_macos::{PermissionKind, PermissionSnapshot, list_running_app_names};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use crate::launcher;
use crate::state::AppState;

#[derive(Serialize)]
pub struct Bootstrap {
    pub needs_onboarding: bool,
    pub computer_approval: ComputerApprovalMode,
    pub launcher_hotkey: HotkeyView,
    pub badges: Vec<String>,
    pub visible: bool,
}

#[derive(Serialize)]
pub struct SettingsView {
    pub base_url: String,
    pub model: String,
    pub vault_path: String,
    pub default_vault_path: String,
    pub provider_key_stored: bool,
    pub exa_key_stored: bool,
    pub permissions: PermissionSnapshot,
    pub computer_approval: ComputerApprovalMode,
    pub launcher_hotkey: HotkeyView,
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
    let config = state.config.load().unwrap_or_default();
    Bootstrap {
        needs_onboarding: !provider_key_is_set(&*state.secrets),
        computer_approval: config.computer_approval,
        launcher_hotkey: config.launcher_hotkey.view(),
        badges: inner.ambient.badge_lines(),
        visible: inner.visible,
    }
}

#[derive(Serialize)]
pub struct StartTaskResult {
    pub task_id: String,
    pub conversation_id: String,
}

#[tauri::command]
pub fn start_task(
    prompt: String,
    mentions: Option<Vec<Mention>>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<StartTaskResult, String> {
    let prompt = prompt.trim().to_string();
    let mentions = mentions.unwrap_or_default();
    if prompt.is_empty() && mentions.is_empty() {
        return Err("prompt is empty".into());
    }
    if !provider_key_is_set(&*state.secrets) {
        return Err(MISSING_API_KEY_MESSAGE.into());
    }
    let task_id = TaskId::new();
    let mut inner = state.lock_inner();
    let conversation_id = inner.conversation_id.unwrap_or_else(ConversationId::new);
    inner.conversation_id = Some(conversation_id);
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
            conversation_id,
            mentions,
        }));
    Ok(StartTaskResult {
        task_id: task_id.to_string(),
        conversation_id: conversation_id.to_string(),
    })
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
        inner.conversation_id = None;
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
        .on_new_window(|url, _| {
            crate::navigation::handle_new_window(&url);
            tauri::webview::NewWindowResponse::Deny
        })
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
    let vault_path = loaded
        .effective_vault_path()
        .unwrap_or_else(default_vault_path)
        .display()
        .to_string();
    SettingsView {
        base_url: loaded.base_url,
        model: loaded.model,
        vault_path,
        default_vault_path: default_vault_path().display().to_string(),
        provider_key_stored,
        exa_key_stored,
        permissions: PermissionSnapshot::current(),
        computer_approval: loaded.computer_approval,
        launcher_hotkey: loaded.launcher_hotkey.view(),
    }
}

#[tauri::command]
pub fn save_config(
    base_url: String,
    model: String,
    vault_path: String,
    state: State<AppState>,
) -> Result<(), String> {
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
    config.vault_path = Some(parse_vault_path_input(&vault_path));
    state.config.save(&config).map_err(|err| err.to_string())?;
    state.commands.send(RuntimeCommand::ReloadKnowledge);
    Ok(())
}

#[tauri::command]
pub fn set_launcher_hotkey(
    spec: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<HotkeyView, String> {
    let parsed = LauncherHotkey::parse(&spec).map_err(|err| err.to_string())?;
    on_main(&app, {
        let parsed = parsed.clone();
        move |state| {
            state.lock_hotkey().set_hotkey(&parsed)?;
            state.lock_inner().capturing_hotkey = false;
            Ok(())
        }
    })?;
    let mut config = state.config.load().unwrap_or_default();
    config.launcher_hotkey = parsed.clone();
    state.config.save(&config).map_err(|err| err.to_string())?;
    Ok(parsed.view())
}

#[tauri::command]
pub fn pause_launcher_hotkey(app: AppHandle) -> Result<(), String> {
    on_main(&app, |state| {
        state.lock_inner().capturing_hotkey = true;
        let result = state.lock_hotkey().clear_hotkey();
        if result.is_err() {
            state.lock_inner().capturing_hotkey = false;
        }
        result
    })
}

#[tauri::command]
pub fn resume_launcher_hotkey(
    app: AppHandle,
    state: State<AppState>,
) -> Result<HotkeyView, String> {
    let parsed = state.config.load().unwrap_or_default().launcher_hotkey;
    on_main(&app, {
        let parsed = parsed.clone();
        move |state| {
            state.lock_hotkey().set_hotkey(&parsed)?;
            state.lock_inner().capturing_hotkey = false;
            Ok(())
        }
    })?;
    Ok(parsed.view())
}

fn on_main<T: Send + 'static>(
    app: &AppHandle,
    op: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = handle
            .try_state::<AppState>()
            .ok_or_else(|| "app not ready".to_string())
            .and_then(|state| op(&state));
        let _ = tx.send(result);
    })
    .map_err(|err| err.to_string())?;
    rx.recv().map_err(|err| err.to_string())?
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
pub fn open_conversation(
    id: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<ConversationView, String> {
    let view = load_conversation(&default_tasks_root(), &id)
        .ok_or_else(|| "conversation not found".to_string())?;
    let parsed: ConversationId = id
        .parse()
        .map_err(|_| "conversation not found".to_string())?;
    let mut artifacts = Vec::new();
    for name in &view.artifact_names {
        if let Some(path) = conversation_artifact_path(&default_tasks_root(), &id, name) {
            artifacts.push((name.clone(), path));
        }
    }
    let mut inner = state.lock_inner();
    inner.conversation_id = Some(parsed);
    inner.current_task = None;
    inner.in_conversation = true;
    inner.compact = false;
    inner.artifacts = artifacts;
    let seq = inner.bump_resize_seq();
    drop(inner);
    launcher::request_resize_with_seq(&app, false, 0, 0.0, seq);
    state.commands.send(RuntimeCommand::ResumeSession(parsed));
    Ok(view)
}

#[tauri::command]
pub fn cycle_computer_approval(state: State<AppState>) -> Result<ComputerApprovalMode, String> {
    let mut config = state.config.load().unwrap_or_default();
    config.computer_approval = config.computer_approval.cycle();
    state.config.save(&config).map_err(|err| err.to_string())?;
    Ok(config.computer_approval)
}

#[tauri::command]
pub fn set_computer_approval(
    mode: ComputerApprovalMode,
    state: State<AppState>,
) -> Result<ComputerApprovalMode, String> {
    let mut config = state.config.load().unwrap_or_default();
    config.computer_approval = mode;
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
    let path = conversation_artifact_path(&default_tasks_root(), &task_id, &name)
        .ok_or_else(|| "artifact not found".to_string())?;
    reveal_in_finder(&path)
}

#[tauri::command]
pub fn set_ui_flags(
    compact: bool,
    composing: bool,
    in_conversation: bool,
    onboarding: bool,
    state: State<AppState>,
) {
    let mut inner = state.lock_inner();
    inner.compact = compact;
    inner.composing = composing;
    inner.in_conversation = in_conversation;
    inner.onboarding = onboarding;
}

#[tauri::command]
pub fn sync_launcher_size(compact: bool, badge_lines: u32, extra_height: f64, app: AppHandle) {
    launcher::request_resize(&app, compact, badge_lines as usize, extra_height);
}

#[tauri::command]
pub fn list_mention_apps() -> Vec<String> {
    list_running_app_names()
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    crate::navigation::open_external_url(&url)
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
