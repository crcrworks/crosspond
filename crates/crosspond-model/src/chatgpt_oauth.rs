use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::ModelError;

pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
pub const SCOPE: &str = "openid profile email offline_access";
pub const CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const CODEX_MODELS_URL: &str = "https://chatgpt.com/backend-api/codex/models";
pub const CODEX_CLIENT_VERSION: &str = "0.144.1";
pub const ORIGINATOR: &str = "codex_cli_rs";
pub const JWT_AUTH_CLAIM: &str = "https://api.openai.com/auth";
const REFRESH_SKEW_MS: i64 = 60_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct ChatGptPkce {
    pub verifier: String,
    pub challenge: String,
}

#[derive(Clone)]
pub struct ChatGptAuthorizationFlow {
    pub pkce: ChatGptPkce,
    pub state: String,
    pub url: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatGptOAuthTokens {
    pub access: String,
    pub refresh: String,
    pub expires_at: i64,
    pub account_id: String,
}

impl std::fmt::Debug for ChatGptOAuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptOAuthTokens")
            .field("access", &"***")
            .field("refresh", &"***")
            .field("expires_at", &self.expires_at)
            .field("account_id", &"***")
            .finish()
    }
}

impl ChatGptOAuthTokens {
    pub fn is_complete(&self) -> bool {
        !self.access.is_empty() && !self.refresh.is_empty() && !self.account_id.is_empty()
    }

    pub fn needs_refresh(&self) -> bool {
        self.expires_at <= unix_ms() + REFRESH_SKEW_MS
    }

    pub fn to_secret_json(&self) -> Result<String, ModelError> {
        serde_json::to_string(self)
            .map_err(|_| ModelError::Network("couldn’t store ChatGPT session".into()))
    }

    pub fn from_secret_json(json: &str) -> Result<Self, ModelError> {
        let tokens: Self = serde_json::from_str(json)
            .map_err(|_| ModelError::Network("stored ChatGPT session is unreadable".into()))?;
        if tokens.is_complete() {
            Ok(tokens)
        } else {
            Err(ModelError::Unauthorized)
        }
    }
}

pub trait ChatGptTokenStore: Send + Sync {
    fn save(&self, tokens: &ChatGptOAuthTokens) -> Result<(), ModelError>;
    fn load(&self) -> Result<Option<ChatGptOAuthTokens>, ModelError>;
}

#[derive(Default)]
pub struct MemoryChatGptTokenStore {
    inner: Mutex<Option<ChatGptOAuthTokens>>,
}

impl MemoryChatGptTokenStore {
    pub fn new(tokens: ChatGptOAuthTokens) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Some(tokens)),
        })
    }

    pub fn current(&self) -> Option<ChatGptOAuthTokens> {
        self.inner
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }
}

impl ChatGptTokenStore for MemoryChatGptTokenStore {
    fn save(&self, tokens: &ChatGptOAuthTokens) -> Result<(), ModelError> {
        *self.inner.lock().unwrap_or_else(|err| err.into_inner()) = Some(tokens.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<ChatGptOAuthTokens>, ModelError> {
        Ok(self.current())
    }
}

pub fn generate_pkce() -> ChatGptPkce {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    ChatGptPkce {
        verifier,
        challenge,
    }
}

pub fn create_authorization_flow() -> ChatGptAuthorizationFlow {
    let pkce = generate_pkce();
    let state = Uuid::new_v4().simple().to_string();
    let url = format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={state}&id_token_add_organizations=true&codex_cli_simplified_flow=true&originator=codex_cli_rs",
        form_encode_value(REDIRECT_URI),
        form_encode_value(SCOPE),
        pkce.challenge
    );
    ChatGptAuthorizationFlow { pkce, state, url }
}

pub fn parse_callback_input(input: &str) -> Result<(String, Option<String>), ModelError> {
    let value = input.trim();
    if value.is_empty() {
        return Err(ModelError::InvalidRequest(
            "missing ChatGPT redirect".into(),
        ));
    }
    if let Ok(url) = parse_query_url(value) {
        let code = query_param(&url, "code")
            .ok_or_else(|| ModelError::InvalidRequest("missing authorization code".into()))?;
        return Ok((code, query_param(&url, "state")));
    }
    if let Some((code, state)) = value.split_once('#') {
        if code.is_empty() {
            return Err(ModelError::InvalidRequest(
                "missing authorization code".into(),
            ));
        }
        return Ok((code.to_string(), Some(state.to_string())));
    }
    Ok((value.to_string(), None))
}

pub async fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    token_url: &str,
) -> Result<ChatGptOAuthTokens, ModelError> {
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("client_id", CLIENT_ID),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
    ]);
    request_tokens(token_url, &body, None).await
}

pub async fn refresh_access_token(
    refresh_token: &str,
    token_url: &str,
) -> Result<ChatGptOAuthTokens, ModelError> {
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ]);
    request_tokens(token_url, &body, Some(refresh_token)).await
}

async fn request_tokens(
    token_url: &str,
    body: &str,
    fallback_refresh: Option<&str>,
) -> Result<ChatGptOAuthTokens, ModelError> {
    let http = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|err| ModelError::Network(err.to_string()))?;
    let response = http
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body.to_string())
        .send()
        .await
        .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ModelError::from_status(status.as_u16(), &text));
    }
    parse_token_response_with_refresh(&text, fallback_refresh)
}

pub fn parse_token_response(body: &str) -> Result<ChatGptOAuthTokens, ModelError> {
    parse_token_response_with_refresh(body, None)
}

pub fn parse_token_response_with_refresh(
    body: &str,
    fallback_refresh: Option<&str>,
) -> Result<ChatGptOAuthTokens, ModelError> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
    }
    let parsed: TokenResponse = serde_json::from_str(body).map_err(|_| ModelError::Unauthorized)?;
    let access = parsed.access_token.filter(|value| !value.is_empty());
    let refresh = parsed
        .refresh_token
        .filter(|value| !value.is_empty())
        .or_else(|| {
            fallback_refresh
                .map(str::to_string)
                .filter(|value| !value.is_empty())
        });
    let expires_in = parsed.expires_in.filter(|value| *value > 0);
    let (Some(access), Some(refresh), Some(expires_in)) = (access, refresh, expires_in) else {
        return Err(ModelError::Unauthorized);
    };
    let account_id = account_id_from_access_token(&access).ok_or(ModelError::Unauthorized)?;
    Ok(ChatGptOAuthTokens {
        access,
        refresh,
        expires_at: unix_ms() + expires_in.saturating_mul(1000),
        account_id,
    })
}

pub fn account_id_from_access_token(access: &str) -> Option<String> {
    let payload = decode_jwt_payload(access)?;
    payload
        .get(JWT_AUTH_CLAIM)
        .and_then(|claim| claim.get("chatgpt_account_id"))
        .or_else(|| payload.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

pub fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn form_encode_value(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn form_body(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{key}={}", form_encode_value(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn parse_query_url(value: &str) -> Result<String, ()> {
    if value.contains("://") || value.starts_with("http") {
        Ok(value.to_string())
    } else if value.contains("code=") || value.contains("error=") {
        Ok(format!("http://localhost/auth/callback?{value}"))
    } else {
        Err(())
    }
}

pub fn chatgpt_refresh_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub async fn refresh_chatgpt_session(
    tokens: &ChatGptOAuthTokens,
    store: &dyn ChatGptTokenStore,
    token_url: &str,
) -> Result<ChatGptOAuthTokens, ModelError> {
    let _guard = chatgpt_refresh_lock().lock().await;
    if let Ok(Some(stored)) = store.load() {
        if !stored.needs_refresh() {
            return Ok(stored);
        }
        let refreshed = refresh_access_token(&stored.refresh, token_url).await?;
        store.save(&refreshed)?;
        return Ok(refreshed);
    }
    if !tokens.needs_refresh() {
        return Ok(tokens.clone());
    }
    let refreshed = refresh_access_token(&tokens.refresh, token_url).await?;
    store.save(&refreshed)?;
    Ok(refreshed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalhostCallback {
    Success { code: String },
    Denied { message: String },
    Ignore,
}

pub fn classify_localhost_http_request(request: &str, expected_state: &str) -> LocalhostCallback {
    let first = request.lines().next().unwrap_or_default();
    let path = first.split_whitespace().nth(1).unwrap_or_default();
    if path.is_empty() {
        return LocalhostCallback::Ignore;
    }
    classify_callback_url(&format!("http://localhost{path}"), expected_state)
}

pub fn classify_callback_url(url: &str, expected_state: &str) -> LocalhostCallback {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return LocalhostCallback::Ignore;
    }
    if let Some((code, state)) = trimmed.split_once('#')
        && !trimmed.contains("://")
        && !trimmed.contains('=')
    {
        if !code.is_empty() && state == expected_state {
            return LocalhostCallback::Success {
                code: code.to_string(),
            };
        }
        return LocalhostCallback::Ignore;
    }
    let parsed = match parse_query_url(trimmed) {
        Ok(value) => value,
        Err(()) => return LocalhostCallback::Ignore,
    };
    if let Some(error) = query_param(&parsed, "error") {
        let Some(state) = query_param(&parsed, "state") else {
            return LocalhostCallback::Ignore;
        };
        if state != expected_state {
            return LocalhostCallback::Ignore;
        }
        let message = if error == "access_denied" {
            "ChatGPT sign-in was cancelled.".into()
        } else {
            "ChatGPT sign-in failed.".into()
        };
        return LocalhostCallback::Denied { message };
    }
    match parse_callback_input(&parsed) {
        Ok((code, Some(state))) if !code.is_empty() && state == expected_state => {
            LocalhostCallback::Success { code }
        }
        _ => LocalhostCallback::Ignore,
    }
}

pub fn code_from_redirect(redirect: &str, expected_state: &str) -> Result<String, String> {
    match classify_callback_url(redirect, expected_state) {
        LocalhostCallback::Success { code } => Ok(code),
        LocalhostCallback::Denied { message } => Err(message),
        LocalhostCallback::Ignore => Err("Paste the full ChatGPT redirect URL.".into()),
    }
}

pub fn wait_for_localhost_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
    cancel: &AtomicBool,
    success_html: &str,
) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|err| err.to_string())?;
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("ChatGPT sign-in cancelled.".into());
        }
        if Instant::now() >= deadline {
            return Err("ChatGPT sign-in timed out.".into());
        }
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(err) => return Err(err.to_string()),
        };
        stream
            .set_nonblocking(false)
            .map_err(|err| err.to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        match classify_localhost_http_request(&request, expected_state) {
            LocalhostCallback::Success { code } => {
                write_http_response(
                    &mut stream,
                    200,
                    "text/html; charset=utf-8",
                    success_html.as_bytes(),
                );
                return Ok(code);
            }
            LocalhostCallback::Denied { message } => {
                write_http_response(&mut stream, 400, "text/plain", b"");
                return Err(message);
            }
            LocalhostCallback::Ignore => {
                write_http_response(&mut stream, 404, "text/plain", b"");
            }
        }
    }
}

fn write_http_response(
    stream: &mut std::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

fn query_param(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or(url);
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        if key == name {
            let value = parts.next().unwrap_or_default();
            return Some(percent_decode(value));
        }
    }
    None
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn public_reqwest_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timed out".into()
    } else if err.is_connect() {
        "couldn’t connect".into()
    } else {
        "network error".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn jwt_with_account(account: &str) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = serde_json::json!({
            JWT_AUTH_CLAIM: { "chatgpt_account_id": account }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn pkce_challenge_is_s256() {
        let pkce = generate_pkce();
        assert!(pkce.verifier.len() >= 43);
        let digest = Sha256::digest(pkce.verifier.as_bytes());
        assert_eq!(
            pkce.challenge,
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
        );
    }

    #[test]
    fn authorize_url_uses_codex_client() {
        let flow = create_authorization_flow();
        assert!(flow.url.starts_with(AUTHORIZE_URL));
        assert!(flow.url.contains(CLIENT_ID));
        assert!(flow.url.contains("code_challenge_method=S256"));
        assert!(flow.url.contains("originator=codex_cli_rs"));
        assert!(!flow.url.contains("sk-"));
    }

    #[test]
    fn parse_callback_from_url_and_hash() {
        let (code, state) =
            parse_callback_input("http://localhost:1455/auth/callback?code=abc%2Fde&state=xyz")
                .unwrap();
        assert_eq!(code, "abc/de");
        assert_eq!(state.as_deref(), Some("xyz"));
        let (code, state) = parse_callback_input("code#state").unwrap();
        assert_eq!(code, "code");
        assert_eq!(state.as_deref(), Some("state"));
    }

    #[test]
    fn debug_redacts_tokens() {
        let tokens = ChatGptOAuthTokens {
            access: "access-secret".into(),
            refresh: "refresh-secret".into(),
            expires_at: 1,
            account_id: "acct_secret".into(),
        };
        let rendered = format!("{tokens:?}");
        assert!(rendered.contains("***"));
        assert!(!rendered.contains("access-secret"));
        assert!(!rendered.contains("refresh-secret"));
        assert!(!rendered.contains("acct_secret"));
    }

    #[test]
    fn secret_json_round_trip() {
        let tokens = ChatGptOAuthTokens {
            access: "a".into(),
            refresh: "r".into(),
            expires_at: 9,
            account_id: "acct".into(),
        };
        let json = tokens.to_secret_json().unwrap();
        let loaded = ChatGptOAuthTokens::from_secret_json(&json).unwrap();
        assert_eq!(loaded.account_id, "acct");
        assert!(json.contains("acct"));
    }

    #[test]
    fn parse_token_response_reads_account_from_jwt() {
        let access = jwt_with_account("acct_123");
        let body = serde_json::json!({
            "access_token": access,
            "refresh_token": "refresh",
            "expires_in": 3600
        })
        .to_string();
        let tokens = parse_token_response(&body).unwrap();
        assert_eq!(tokens.account_id, "acct_123");
        assert_eq!(tokens.refresh, "refresh");
        assert!(tokens.expires_at > unix_ms());
    }

    #[test]
    fn parse_token_response_requires_refresh() {
        let access = jwt_with_account("acct_123");
        let body = serde_json::json!({
            "access_token": access,
            "expires_in": 3600
        })
        .to_string();
        assert!(matches!(
            parse_token_response(&body),
            Err(ModelError::Unauthorized)
        ));
    }

    #[test]
    fn parse_refresh_response_keeps_old_refresh_token() {
        let access = jwt_with_account("acct_123");
        let body = serde_json::json!({
            "access_token": access,
            "expires_in": 3600
        })
        .to_string();
        let tokens = parse_token_response_with_refresh(&body, Some("old-refresh")).unwrap();
        assert_eq!(tokens.refresh, "old-refresh");
        assert_eq!(tokens.account_id, "acct_123");
    }

    #[test]
    fn callback_requires_matching_state() {
        assert!(matches!(
            classify_callback_url(
                "http://localhost:1455/auth/callback?code=abc&state=good",
                "good"
            ),
            LocalhostCallback::Success { code } if code == "abc"
        ));
        assert_eq!(
            classify_callback_url(
                "http://localhost:1455/auth/callback?code=abc&state=other",
                "good"
            ),
            LocalhostCallback::Ignore
        );
        assert_eq!(
            classify_callback_url("http://localhost:1455/auth/callback?code=abc", "good"),
            LocalhostCallback::Ignore
        );
        assert_eq!(
            code_from_redirect("abc", "good").unwrap_err(),
            "Paste the full ChatGPT redirect URL."
        );
        assert_eq!(
            classify_localhost_http_request("GET /favicon.ico HTTP/1.1", "good"),
            LocalhostCallback::Ignore
        );
        assert!(matches!(
            classify_callback_url(
                "http://localhost:1455/auth/callback?error=access_denied&state=good",
                "good"
            ),
            LocalhostCallback::Denied { .. }
        ));
    }

    #[test]
    fn wait_ignores_favicon_then_accepts_callback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_wait = Arc::clone(&cancel);
        let handle = thread::spawn(move || {
            wait_for_localhost_callback(
                listener,
                "state-1",
                Duration::from_secs(5),
                cancel_wait.as_ref(),
                "<p>ok</p>",
            )
        });
        let mut favicon = std::net::TcpStream::connect(addr).unwrap();
        favicon
            .write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let _ = favicon.read(&mut [0u8; 256]);
        let mut callback = std::net::TcpStream::connect(addr).unwrap();
        callback
            .write_all(
                b"GET /auth/callback?code=the-code&state=state-1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
            )
            .unwrap();
        let _ = callback.read(&mut [0u8; 512]);
        assert_eq!(handle.join().unwrap().unwrap(), "the-code");
        let _ = cancel;
    }

    #[test]
    fn wait_stops_when_cancelled() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_wait = Arc::clone(&cancel);
        let handle = thread::spawn(move || {
            wait_for_localhost_callback(
                listener,
                "state-1",
                Duration::from_secs(5),
                cancel_wait.as_ref(),
                "<p>ok</p>",
            )
        });
        cancel.store(true, Ordering::SeqCst);
        let err = handle.join().unwrap().unwrap_err();
        assert!(err.contains("cancelled"));
    }

    #[tokio::test]
    async fn exchange_code_hits_token_url() {
        let access = jwt_with_account("acct_live");
        let body = serde_json::json!({
            "access_token": access,
            "refresh_token": "refresh-live",
            "expires_in": 10
        })
        .to_string();
        let url = serve_json(200, &body);
        let tokens = exchange_authorization_code("code", "verifier", REDIRECT_URI, &url)
            .await
            .unwrap();
        assert_eq!(tokens.account_id, "acct_live");
        assert_eq!(tokens.refresh, "refresh-live");
    }

    #[tokio::test]
    async fn refresh_maps_401() {
        let url = serve_json(401, r#"{"error":"invalid_grant"}"#);
        let err = refresh_access_token("refresh", &url).await.unwrap_err();
        assert!(matches!(err, ModelError::Unauthorized));
        assert!(!err.user_message().contains("invalid_grant"));
    }

    #[tokio::test]
    async fn refresh_session_keeps_caller_tokens_when_store_is_fresh() {
        let tokens = ChatGptOAuthTokens {
            access: "a".into(),
            refresh: "r".into(),
            expires_at: unix_ms() + 3_600_000,
            account_id: "acct".into(),
        };
        let store = MemoryChatGptTokenStore::new(tokens.clone());
        let refreshed =
            refresh_chatgpt_session(&tokens, store.as_ref(), "http://127.0.0.1:1/unused")
                .await
                .unwrap_or(tokens);
        assert_eq!(refreshed.access, "a");
        assert_eq!(refreshed.account_id, "acct");
    }

    fn serve_json(status: u16, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let header = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(body.as_bytes()).unwrap();
        });
        format!("http://{addr}/oauth/token")
    }
}
