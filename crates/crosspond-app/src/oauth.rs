use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crosspond_core::{
    DEFAULT_CHATGPT_MODEL, DEFAULT_COMPAT_MODEL, REDIRECT_URI, SelectedModel, TOKEN_URL,
    code_from_redirect, create_authorization_flow, exchange_authorization_code,
    save_chatgpt_tokens, wait_for_localhost_callback,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::state::{AppState, PendingChatGptLogin};

const CALLBACK_PORT: u16 = 1455;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
const BIND_RETRY: u32 = 25;
const BIND_RETRY_WAIT: Duration = Duration::from_millis(40);
const SUCCESS_HTML: &str = "<!doctype html><html><body style=\"font-family:sans-serif;padding:2rem\"><p>Signed in to Crosspond. You can close this window.</p></body></html>";

#[derive(Serialize)]
pub struct ChatGptLoginStart {
    pub mode: String,
    pub authorize_url: String,
}

pub fn start_login(app: &AppHandle, state: &AppState) -> Result<ChatGptLoginStart, String> {
    let had_pending = state.lock_pending_chatgpt().is_some();
    cancel_pending(state);

    let flow = create_authorization_flow();
    let cancel = Arc::new(AtomicBool::new(false));
    *state.lock_pending_chatgpt() = Some(PendingChatGptLogin {
        verifier: flow.pkce.verifier.clone(),
        state: flow.state.clone(),
        cancel: Arc::clone(&cancel),
    });

    match bind_callback_listener(had_pending) {
        Ok(listener) => {
            let _ = app.opener().open_url(&flow.url, None::<&str>);
            let app = app.clone();
            let verifier = flow.pkce.verifier.clone();
            let expected_state = flow.state.clone();
            std::thread::Builder::new()
                .name("crosspond-chatgpt-oauth".into())
                .spawn(move || {
                    let result = wait_for_localhost_callback(
                        listener,
                        &expected_state,
                        CALLBACK_TIMEOUT,
                        cancel.as_ref(),
                        SUCCESS_HTML,
                    )
                    .and_then(|code| {
                        if cancel.load(Ordering::SeqCst) {
                            return Err("ChatGPT sign-in cancelled.".into());
                        }
                        exchange_and_store(&app, &code, &verifier)
                    });
                    if !cancel.load(Ordering::SeqCst) {
                        emit_login_result(&app, result);
                    }
                })
                .map_err(|err| err.to_string())?;
            Ok(ChatGptLoginStart {
                mode: "browser".into(),
                authorize_url: flow.url,
            })
        }
        Err(_) => Ok(ChatGptLoginStart {
            mode: "manual".into(),
            authorize_url: flow.url,
        }),
    }
}

pub fn complete_login(app: &AppHandle, state: &AppState, redirect: &str) -> Result<(), String> {
    let pending = state
        .lock_pending_chatgpt()
        .take()
        .ok_or_else(|| "start ChatGPT sign-in first".to_string())?;
    pending.cancel.store(true, Ordering::SeqCst);
    let code = code_from_redirect(redirect, &pending.state)?;
    exchange_and_store(app, &code, &pending.verifier)
}

pub fn cancel_login(state: &AppState) {
    cancel_pending(state);
}

pub fn sign_out(app: &AppHandle, state: &AppState) -> Result<(), String> {
    cancel_pending(state);
    state
        .secrets
        .delete(&crosspond_core::SecretKey::chatgpt_oauth())
        .map_err(|err| err.to_string())?;
    bump_models(app, state);
    let mut config = state.config.load().unwrap_or_default();
    if config.selected.is_chatgpt() {
        if let Some(endpoint) = config.openai_compat.first() {
            config.selected = SelectedModel::compat(&endpoint.id, DEFAULT_COMPAT_MODEL);
            state.config.save(&config).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn cancel_pending(state: &AppState) {
    if let Some(pending) = state.lock_pending_chatgpt().take() {
        pending.cancel.store(true, Ordering::SeqCst);
    }
}

fn bind_callback_listener(retry: bool) -> Result<TcpListener, std::io::Error> {
    let attempts = if retry { BIND_RETRY } else { 1 };
    let mut last = None;
    for attempt in 0..attempts {
        match TcpListener::bind(("127.0.0.1", CALLBACK_PORT)) {
            Ok(listener) => return Ok(listener),
            Err(err) => {
                last = Some(err);
                if attempt + 1 < attempts {
                    std::thread::sleep(BIND_RETRY_WAIT);
                }
            }
        }
    }
    Err(last.expect("bind attempted"))
}

fn exchange_and_store(app: &AppHandle, code: &str, verifier: &str) -> Result<(), String> {
    let tokens = tauri::async_runtime::block_on(exchange_authorization_code(
        code,
        verifier,
        REDIRECT_URI,
        TOKEN_URL,
    ))
    .map_err(|err| err.user_message())?;
    let Some(state) = app.try_state::<AppState>() else {
        return Err("app not ready".into());
    };
    save_chatgpt_tokens(&*state.secrets, &tokens).map_err(|err| err.to_string())?;
    *state.lock_pending_chatgpt() = None;
    bump_models(app, &state);
    let mut config = state.config.load().unwrap_or_default();
    if !config.selected.is_chatgpt() && config.selected.model == DEFAULT_COMPAT_MODEL {
        config.selected = SelectedModel::chatgpt(DEFAULT_CHATGPT_MODEL);
        state.config.save(&config).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub(crate) fn bump_models(app: &AppHandle, state: &AppState) {
    state.invalidate_models();
    let _ = app.emit("models-changed", ());
}

fn emit_login_result(app: &AppHandle, result: Result<(), String>) {
    let (ok, message) = match result {
        Ok(()) => (true, "Signed in with ChatGPT.".to_string()),
        Err(message) => (false, message),
    };
    let _ = app.emit("chatgpt-login", ChatGptLoginEvent { ok, message });
}

#[derive(Serialize, Clone)]
struct ChatGptLoginEvent {
    ok: bool,
    message: String,
}
