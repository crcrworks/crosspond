use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::chatgpt_oauth::{
    CODEX_CLIENT_VERSION, CODEX_MODELS_URL, ChatGptOAuthTokens, ORIGINATOR,
};
use crate::error::ModelError;
use crate::openai_compat::models_url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ListedModel {
    pub id: String,
    pub label: String,
}

pub fn fallback_chatgpt_models() -> Vec<ListedModel> {
    ["gpt-5.6-luna", "gpt-5.2", "gpt-5.1-codex", "gpt-5"]
        .into_iter()
        .map(listed)
        .collect()
}

pub fn fallback_compat_models() -> Vec<ListedModel> {
    ["gpt-4o-mini", "gpt-4o", "gpt-4.1"]
        .into_iter()
        .map(listed)
        .collect()
}

fn listed(id: &str) -> ListedModel {
    ListedModel {
        id: id.to_string(),
        label: id.to_string(),
    }
}

pub fn parse_models_json(body: &str) -> Result<Vec<ListedModel>, ModelError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ModelError::EmptyResponse)?;
    let mut models = collect_models(&value);
    models.retain(|model| is_safe_model_id(&model.id));
    models.dedup_by(|a, b| a.id == b.id);
    if models.is_empty() {
        Err(ModelError::EmptyResponse)
    } else {
        Ok(models)
    }
}

fn collect_models(value: &Value) -> Vec<ListedModel> {
    let mut out = Vec::new();
    if let Some(arr) = value.get("data").and_then(Value::as_array) {
        for item in arr {
            push_model(&mut out, item);
        }
    }
    if out.is_empty()
        && let Some(arr) = value.get("models").and_then(Value::as_array)
    {
        for item in arr {
            push_model(&mut out, item);
        }
    }
    if out.is_empty()
        && let Some(arr) = value.as_array()
    {
        for item in arr {
            push_model(&mut out, item);
        }
    }
    out
}

fn push_model(out: &mut Vec<ListedModel>, item: &Value) {
    if let Some(id) = item.as_str() {
        if is_safe_model_id(id) {
            out.push(listed(id));
        }
        return;
    }
    let id = item
        .get("id")
        .or_else(|| item.get("slug"))
        .or_else(|| item.get("model"))
        .and_then(Value::as_str);
    let Some(id) = id.filter(|value| is_safe_model_id(value)) else {
        return;
    };
    let label = item
        .get("title")
        .or_else(|| item.get("label"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() < 200)
        .unwrap_or(id);
    out.push(ListedModel {
        id: id.to_string(),
        label: label.to_string(),
    });
}

fn is_safe_model_id(id: &str) -> bool {
    let trimmed = id.trim();
    !trimmed.is_empty()
        && trimmed.len() < 128
        && !trimmed.starts_with("sk-")
        && !trimmed.starts_with("eyJ")
        && !trimmed.contains("access_token")
        && !trimmed.contains("refresh_token")
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_graphic() && ch != '"' && ch != '\\')
}

pub fn ensure_model(models: &mut Vec<ListedModel>, id: &str) {
    let id = id.trim();
    if id.is_empty() || !is_safe_model_id(id) {
        return;
    }
    if models.iter().any(|model| model.id == id) {
        return;
    }
    models.push(listed(id));
}

pub async fn fetch_compat_models(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ListedModel>, ModelError> {
    fetch_compat_models_at(&models_url(base_url), api_key).await
}

pub async fn fetch_compat_models_at(
    url: &str,
    api_key: &str,
) -> Result<Vec<ListedModel>, ModelError> {
    let http = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|err| ModelError::Network(err.to_string()))?;
    let response = http
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ModelError::from_status(status.as_u16(), &text));
    }
    parse_models_json(&text)
}

pub async fn fetch_chatgpt_models(
    tokens: &ChatGptOAuthTokens,
) -> Result<Vec<ListedModel>, ModelError> {
    let url = format!("{CODEX_MODELS_URL}?client_version={CODEX_CLIENT_VERSION}");
    fetch_chatgpt_models_at(&url, tokens).await
}

pub async fn fetch_chatgpt_models_at(
    url: &str,
    tokens: &ChatGptOAuthTokens,
) -> Result<Vec<ListedModel>, ModelError> {
    let http = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|err| ModelError::Network(err.to_string()))?;
    let response = http
        .get(url)
        .bearer_auth(&tokens.access)
        .header("chatgpt-account-id", &tokens.account_id)
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", ORIGINATOR)
        .header("version", CODEX_CLIENT_VERSION)
        .send()
        .await
        .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ModelError::from_status(status.as_u16(), &text));
    }
    parse_models_json(&text)
}

fn public_reqwest_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timed out".into()
    } else if err.is_connect() {
        "connection failed".into()
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

    #[test]
    fn parses_openai_models_list_without_tokens() {
        let json = r#"{
            "object":"list",
            "access_token":"sk-secret-should-not-leak",
            "data":[
                {"id":"gpt-4o-mini","object":"model"},
                {"id":"gpt-4o","owned_by":"openai"}
            ]
        }"#;
        let models = parse_models_json(json).unwrap();
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["gpt-4o-mini", "gpt-4o"]
        );
        let rendered = serde_json::to_string(&models).unwrap();
        assert!(!rendered.contains("sk-"));
        assert!(!rendered.contains("access_token"));
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn parses_codex_models_with_slug_and_title() {
        let json = r#"{
            "models":[
                {"slug":"gpt-5.6-luna","title":"GPT-5.6 Luna"},
                {"id":"gpt-5.2"}
            ]
        }"#;
        let models = parse_models_json(json).unwrap();
        assert_eq!(models[0].id, "gpt-5.6-luna");
        assert_eq!(models[0].label, "GPT-5.6 Luna");
        assert_eq!(models[1].id, "gpt-5.2");
    }

    #[test]
    fn skips_jwt_and_api_key_shaped_ids() {
        let json = r#"{"data":[
            {"id":"eyJhbGciOiJIUzI1NiJ9.payload.sig"},
            {"id":"sk-live-secret"},
            {"id":"gpt-5.6-luna"}
        ]}"#;
        let models = parse_models_json(json).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.6-luna");
        let rendered = serde_json::to_string(&models).unwrap();
        assert!(!rendered.contains("eyJ"));
        assert!(!rendered.contains("sk-live"));
    }

    #[tokio::test]
    async fn fetch_compat_models_from_mock_http() {
        let body = br#"{"data":[{"id":"qwen2.5"}]}"#;
        let (base, _captured) = serve_http(200, body, None);
        let models = fetch_compat_models(&base, "sk-test").await.unwrap();
        assert_eq!(models[0].id, "qwen2.5");
    }

    #[tokio::test]
    async fn fetch_chatgpt_models_sends_version_header() {
        let body = br#"{"data":[{"id":"gpt-5.6-luna"}]}"#;
        let (base, captured) = serve_http(200, body, Some("GET"));
        let tokens = ChatGptOAuthTokens {
            access: "access-token".into(),
            refresh: "refresh-token".into(),
            expires_at: 9_999_999_999_999,
            account_id: "acct_1".into(),
        };
        let models = fetch_chatgpt_models_at(&format!("{base}/codex/models"), &tokens)
            .await
            .unwrap();
        assert_eq!(models[0].id, "gpt-5.6-luna");
        let request = captured.lock().expect("capture").clone();
        assert!(
            request
                .to_ascii_lowercase()
                .contains("originator: codex_cli_rs")
        );
        assert!(request.to_ascii_lowercase().contains("version: 0.144.1"));
        assert!(
            !serde_json::to_string(&models)
                .unwrap()
                .contains("access-token")
        );
    }

    fn serve_http(
        status: u16,
        body: &[u8],
        _method: Option<&str>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<String>>) {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_vec();
        let captured_thread = std::sync::Arc::clone(&captured);
        thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            *captured_thread.lock().expect("capture") =
                String::from_utf8_lossy(&buf[..n]).to_string();
            let header = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        });
        (format!("http://{addr}/v1"), captured)
    }
}
