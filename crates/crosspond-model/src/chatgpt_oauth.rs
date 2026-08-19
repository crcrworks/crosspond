use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    request_tokens(token_url, &body).await
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
    request_tokens(token_url, &body).await
}

async fn request_tokens(token_url: &str, body: &str) -> Result<ChatGptOAuthTokens, ModelError> {
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
    parse_token_response(&text)
}

pub fn parse_token_response(body: &str) -> Result<ChatGptOAuthTokens, ModelError> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
    }
    let parsed: TokenResponse = serde_json::from_str(body).map_err(|_| ModelError::Unauthorized)?;
    let access = parsed.access_token.filter(|value| !value.is_empty());
    let refresh = parsed.refresh_token.filter(|value| !value.is_empty());
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
    } else if value.contains("code=") {
        Ok(format!("http://localhost/auth/callback?{value}"))
    } else {
        Err(())
    }
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
