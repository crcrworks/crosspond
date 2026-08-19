use std::path::{Path, PathBuf};
use std::process::Command;

use crosspond_core::{
    AppConfig, ApprovalId, CHATGPT_SOURCE, ComputerApprovalMode, ConversationId, ConversationView,
    DEFAULT_CHATGPT_MODEL, DEFAULT_COMPAT_ID, DEFAULT_COMPAT_MODEL, HotkeyView, LauncherHotkey,
    ListedModel, MISSING_API_KEY_MESSAGE, MISSING_CHATGPT_MESSAGE, Mention, ReasoningEffort,
    Receipt, RuntimeCommand, SecretKey, SecretString, SelectedModel, StartTaskRequest, TaskId,
    conversation_artifact_path, default_tasks_root, default_vault_path, ensure_model,
    fallback_chatgpt_models, fallback_compat_models, fetch_chatgpt_models, fetch_compat_models,
    history_group_label, history_title, list_recent_tasks, load_chatgpt_tokens,
    open_conversation as load_conversation, parse_vault_path_input, provider_is_ready,
    selected_provider_is_ready,
};
use crosspond_macos::{PermissionKind, PermissionSnapshot, list_running_app_names};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use crate::launcher;
use crate::state::AppState;

#[derive(Serialize, Clone)]
pub struct SelectedView {
    pub source: String,
    pub model: String,
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub needs_onboarding: bool,
    pub computer_approval: ComputerApprovalMode,
    pub launcher_hotkey: HotkeyView,
    pub badges: Vec<String>,
    pub visible: bool,
    pub selected: SelectedView,
    pub reasoning_effort: String,
}

#[derive(Serialize, Clone)]
pub struct CompatEndpointView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub key_stored: bool,
}

#[derive(Serialize)]
pub struct SettingsView {
    pub openai_compat: Vec<CompatEndpointView>,
    pub selected: SelectedView,
    pub reasoning_effort: String,
    pub vault_path: String,
    pub default_vault_path: String,
    pub chatgpt_signed_in: bool,
    pub provider_ready: bool,
    pub selected_ready: bool,
    pub exa_key_stored: bool,
    pub permissions: PermissionSnapshot,
    pub computer_approval: ComputerApprovalMode,
    pub launcher_hotkey: HotkeyView,
}

#[derive(Serialize, Clone)]
pub struct ListedModelView {
    pub id: String,
    pub label: String,
}

#[derive(Serialize, Clone)]
pub struct ModelGroupView {
    pub source: String,
    pub label: String,
    pub models: Vec<ListedModelView>,
}

#[derive(Serialize, Clone)]
pub struct ModelsCatalog {
    pub groups: Vec<ModelGroupView>,
    pub selected: SelectedView,
    pub reasoning_effort: String,
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
        needs_onboarding: !provider_is_ready(&config, &*state.secrets),
        computer_approval: config.computer_approval,
        launcher_hotkey: config.launcher_hotkey.view(),
        badges: inner.ambient.badge_lines(),
        visible: inner.visible,
        selected: selected_view(&config),
        reasoning_effort: config.reasoning_effort.as_str().into(),
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
    let config = state.config.load().unwrap_or_default();
    if !selected_provider_is_ready(&config, &*state.secrets) {
        return Err(if config.selected.is_chatgpt() {
            MISSING_CHATGPT_MESSAGE.into()
        } else {
            MISSING_API_KEY_MESSAGE.into()
        });
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
        .inner_size(520.0, 720.0)
        .min_inner_size(420.0, 520.0)
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
    settings_view(&state)
}

fn settings_view(state: &AppState) -> SettingsView {
    let loaded = state.config.load().unwrap_or_default();
    let exa_key_stored = state
        .secrets
        .get(&SecretKey::exa_api_key())
        .ok()
        .flatten()
        .is_some_and(|key| !key.is_empty());
    let vault_path = loaded
        .effective_vault_path()
        .unwrap_or_else(default_vault_path)
        .display()
        .to_string();
    let chatgpt_signed_in = crosspond_core::chatgpt_oauth_is_set(&*state.secrets);
    SettingsView {
        openai_compat: loaded
            .openai_compat
            .iter()
            .map(|endpoint| CompatEndpointView {
                id: endpoint.id.clone(),
                name: endpoint.name.clone(),
                base_url: endpoint.base_url.clone(),
                key_stored: crosspond_core::compat_key_is_set(&endpoint.id, &*state.secrets),
            })
            .collect(),
        selected: selected_view(&loaded),
        reasoning_effort: loaded.reasoning_effort.as_str().into(),
        vault_path,
        default_vault_path: default_vault_path().display().to_string(),
        chatgpt_signed_in,
        provider_ready: provider_is_ready(&loaded, &*state.secrets),
        selected_ready: selected_provider_is_ready(&loaded, &*state.secrets),
        exa_key_stored,
        permissions: PermissionSnapshot::current(),
        computer_approval: loaded.computer_approval,
        launcher_hotkey: loaded.launcher_hotkey.view(),
    }
}

fn selected_view(config: &AppConfig) -> SelectedView {
    SelectedView {
        source: config.selected.source.clone(),
        model: config.selected.model.clone(),
    }
}

fn save_loaded_config(state: &AppState, mut config: AppConfig) -> Result<(), String> {
    config.normalize();
    state.config.save(&config).map_err(|err| err.to_string())?;
    state.invalidate_models();
    Ok(())
}

#[tauri::command]
pub fn save_config(vault_path: String, state: State<AppState>) -> Result<(), String> {
    let mut config = state.config.load().unwrap_or_default();
    config.vault_path = Some(parse_vault_path_input(&vault_path));
    save_loaded_config(&state, config)?;
    state.commands.send(RuntimeCommand::ReloadKnowledge);
    Ok(())
}

#[tauri::command]
pub fn save_compat(
    id: String,
    name: String,
    base_url: String,
    state: State<AppState>,
) -> Result<SettingsView, String> {
    let mut config = state.config.load().unwrap_or_default();
    let id = crosspond_core::sanitize_compat_id(&id);
    let Some(endpoint) = config
        .openai_compat
        .iter_mut()
        .find(|endpoint| endpoint.id == id)
    else {
        return Err("Unknown OpenAI Compatible endpoint.".into());
    };
    if !name.trim().is_empty() {
        endpoint.name = name.trim().to_string();
    }
    endpoint.base_url = if base_url.trim().is_empty() {
        "https://api.openai.com/v1".into()
    } else {
        base_url.trim().to_string()
    };
    save_loaded_config(&state, config)?;
    Ok(settings_view(&state))
}

#[tauri::command]
pub fn add_compat(state: State<AppState>) -> Result<SettingsView, String> {
    let mut config = state.config.load().unwrap_or_default();
    config.add_compat();
    save_loaded_config(&state, config)?;
    Ok(settings_view(&state))
}

#[tauri::command]
pub fn delete_compat(id: String, state: State<AppState>) -> Result<SettingsView, String> {
    let mut config = state.config.load().unwrap_or_default();
    if !config.remove_compat(&id) {
        return Err("Keep at least one OpenAI Compatible endpoint.".into());
    }
    let _ = state.secrets.delete(&SecretKey::provider_api_key_for(&id));
    save_loaded_config(&state, config)?;
    Ok(settings_view(&state))
}

#[tauri::command]
pub fn save_selected(
    source: String,
    model: String,
    state: State<AppState>,
) -> Result<SelectedView, String> {
    let mut config = state.config.load().unwrap_or_default();
    let model = if model.trim().is_empty() {
        if source == CHATGPT_SOURCE {
            DEFAULT_CHATGPT_MODEL.to_string()
        } else {
            DEFAULT_COMPAT_MODEL.to_string()
        }
    } else {
        model.trim().to_string()
    };
    if source == CHATGPT_SOURCE {
        config.selected = SelectedModel::chatgpt(model);
    } else if config.compat(&source).is_some() {
        config.selected = SelectedModel::compat(source, model);
    } else {
        return Err("Unknown model source.".into());
    }
    let selected = selected_view(&config);
    save_loaded_config(&state, config)?;
    Ok(selected)
}

#[tauri::command]
pub fn save_effort(effort: String, state: State<AppState>) -> Result<String, String> {
    let mut config = state.config.load().unwrap_or_default();
    config.reasoning_effort = ReasoningEffort::parse(&effort);
    let stored = config.reasoning_effort.as_str().to_string();
    save_loaded_config(&state, config)?;
    Ok(stored)
}

const MODELS_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> Result<ModelsCatalog, String> {
    {
        let cache = state
            .models_cache
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some(entry) = cache.as_ref()
            && entry.at.elapsed() < MODELS_TTL
        {
            return Ok(entry.catalog.clone());
        }
    }
    let config = state.config.load().unwrap_or_default();
    let secrets = std::sync::Arc::clone(&state.secrets);
    let catalog = collect_models_catalog(&config, secrets.as_ref()).await;
    *state
        .models_cache
        .lock()
        .unwrap_or_else(|err| err.into_inner()) = Some(crate::state::ModelsCacheEntry {
        at: std::time::Instant::now(),
        catalog: catalog.clone(),
    });
    Ok(catalog)
}

async fn collect_models_catalog(
    config: &AppConfig,
    secrets: &dyn crosspond_core::SecretStore,
) -> ModelsCatalog {
    let mut groups = Vec::new();
    if let Ok(Some(tokens)) = load_chatgpt_tokens(secrets) {
        let mut models = match fetch_chatgpt_models(&tokens).await {
            Ok(models) if !models.is_empty() => models,
            _ => fallback_chatgpt_models(),
        };
        if config.selected.is_chatgpt() {
            ensure_model(&mut models, &config.selected.model);
        }
        groups.push(model_group(CHATGPT_SOURCE, "ChatGPT", models));
    }
    for endpoint in &config.openai_compat {
        let mut models = if let Some(key) = secrets
            .get(&SecretKey::provider_api_key_for(&endpoint.id))
            .ok()
            .flatten()
            .filter(|key| !key.is_empty())
        {
            match fetch_compat_models(&endpoint.base_url, key.expose()).await {
                Ok(models) if !models.is_empty() => models,
                _ => fallback_compat_models(),
            }
        } else {
            fallback_compat_models()
        };
        if config.selected.source == endpoint.id {
            ensure_model(&mut models, &config.selected.model);
        }
        groups.push(model_group(&endpoint.id, &endpoint.name, models));
    }
    ModelsCatalog {
        groups,
        selected: selected_view(config),
        reasoning_effort: config.reasoning_effort.as_str().into(),
    }
}

fn model_group(source: &str, label: &str, models: Vec<ListedModel>) -> ModelGroupView {
    ModelGroupView {
        source: source.into(),
        label: label.into(),
        models: models
            .into_iter()
            .map(|model| ListedModelView {
                id: model.id,
                label: model.label,
            })
            .collect(),
    }
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
    let key = secret_key_for_kind(&kind)?;
    state
        .secrets
        .set(&key, &SecretString::new(value))
        .map_err(|err| err.to_string())?;
    state.invalidate_models();
    Ok(())
}

fn secret_key_for_kind(kind: &str) -> Result<SecretKey, String> {
    match kind {
        "exa" => Ok(SecretKey::exa_api_key()),
        "provider" => Ok(SecretKey::provider_api_key_for(DEFAULT_COMPAT_ID)),
        other => {
            if let Some(id) = other.strip_prefix("provider.") {
                Ok(SecretKey::provider_api_key_for(id))
            } else {
                Err("unknown secret".into())
            }
        }
    }
}

#[tauri::command]
pub fn start_chatgpt_login(
    app: AppHandle,
    state: State<AppState>,
) -> Result<crate::oauth::ChatGptLoginStart, String> {
    crate::oauth::start_login(&app, &state)
}

#[tauri::command]
pub fn complete_chatgpt_login(
    redirect: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<(), String> {
    crate::oauth::complete_login(&app, &state, &redirect)
}

#[tauri::command]
pub fn sign_out_chatgpt(state: State<AppState>) -> Result<(), String> {
    crate::oauth::sign_out(&state)
}

#[tauri::command]
pub fn test_connection(state: State<AppState>) {
    state.commands.send(RuntimeCommand::TestConnection);
}

#[tauri::command]
pub fn test_compat_connection(id: String, state: State<AppState>) {
    state.commands.send(RuntimeCommand::TestCompat { id });
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
