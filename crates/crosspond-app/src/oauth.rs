use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use crosspond_core::{
    AppConfig, ProviderKind, REDIRECT_URI, TOKEN_URL, create_authorization_flow,
    exchange_authorization_code, parse_callback_input, save_chatgpt_tokens,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::state::{AppState, PendingChatGptLogin};

const CALLBACK_PORT: u16 = 1455;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
const SUCCESS_HTML: &str = "<!doctype html><html><body style=\"font-family:sans-serif;padding:2rem\"><p>Signed in to Crosspond. You can close this window.</p></body></html>";

#[derive(Serialize)]
pub struct ChatGptLoginStart {
    pub mode: String,
    pub authorize_url: String,
}

pub fn start_login(app: &AppHandle, state: &AppState) -> Result<ChatGptLoginStart, String> {
    let flow = create_authorization_flow();
    *state.lock_pending_chatgpt() = Some(PendingChatGptLogin {
        verifier: flow.pkce.verifier.clone(),
        state: flow.state.clone(),
    });

    match TcpListener::bind(("127.0.0.1", CALLBACK_PORT)) {
        Ok(listener) => {
            let _ = app.opener().open_url(&flow.url, None::<&str>);
            let app = app.clone();
            let verifier = flow.pkce.verifier.clone();
            let expected_state = flow.state.clone();
            std::thread::Builder::new()
                .name("crosspond-chatgpt-oauth".into())
                .spawn(move || {
                    let result = wait_for_code(listener, &expected_state)
                        .and_then(|code| exchange_and_store(&app, &code, &verifier));
                    emit_login_result(&app, result);
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
    let (code, state_value) = parse_callback_input(redirect).map_err(|err| err.user_message())?;
    if let Some(got) = state_value
        && got != pending.state
    {
        return Err("ChatGPT sign-in state did not match".into());
    }
    exchange_and_store(app, &code, &pending.verifier)
}

pub fn sign_out(state: &AppState) -> Result<(), String> {
    state
        .secrets
        .delete(&crosspond_core::SecretKey::CHATGPT_OAUTH)
        .map_err(|err| err.to_string())?;
    *state.lock_pending_chatgpt() = None;
    let mut config = state.config.load().unwrap_or_default();
    if config.provider == ProviderKind::ChatGptCodex {
        config.provider = ProviderKind::OpenaiCompatible;
        state.config.save(&config).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|err| err.to_string())?;
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(accepted) => break accepted,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if Instant::now() >= deadline {
                    return Err("ChatGPT sign-in timed out".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err.to_string()),
        }
    };
    stream
        .set_nonblocking(false)
        .map_err(|err| err.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]);
    let first = request.lines().next().unwrap_or_default();
    let path = first.split_whitespace().nth(1).unwrap_or_default();
    let redirect = format!("http://localhost:{CALLBACK_PORT}{path}");
    let (code, state) = parse_callback_input(&redirect).map_err(|err| err.user_message())?;
    if let Some(got) = state
        && got != expected_state
    {
        let _ = stream.write_all(
            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return Err("ChatGPT sign-in state did not match".into());
    }
    let body = SUCCESS_HTML.as_bytes();
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    Ok(code)
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
    let mut config = state.config.load().unwrap_or_default();
    config.provider = ProviderKind::ChatGptCodex;
    if config.model == AppConfig::default().model {
        config.model = crosspond_core::DEFAULT_CHATGPT_MODEL.into();
    }
    state.config.save(&config).map_err(|err| err.to_string())?;
    Ok(())
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

pub fn apply_saved_provider(config: &mut AppConfig, provider: &str) {
    match provider {
        "chatgpt_codex" => {
            if config.model == AppConfig::default().model {
                config.model = crosspond_core::DEFAULT_CHATGPT_MODEL.into();
            }
            config.provider = ProviderKind::ChatGptCodex;
        }
        _ => config.provider = ProviderKind::OpenaiCompatible,
    }
}
