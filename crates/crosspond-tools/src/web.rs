//! Web search (Exa) and page fetch tools.
//!
//! Search provider is Exa for now; Settings / Keychain can grow a provider choice later.

use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder};
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::browser::{host_from_url, normalize_host};
use crate::registry::ToolRegistry;
use crate::ssrf::{
    max_redirects, validate_fetch_url, validate_fetch_url_for_hosts, validate_url,
    validate_url_allowing_private,
};
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult, truncate_output};

const EXA_SEARCH_URL: &str = "https://api.exa.ai/search";
const MISSING_EXA_KEY: &str = "Add an Exa API key in Settings (⌘,) before using web_search.";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const DEFAULT_COUNT: u64 = 5;
const MAX_COUNT: u64 = 10;
const SNIPPET_MAX_CHARS: usize = 500;

pub fn register_web_tools(registry: &mut ToolRegistry) {
    registry.register(Arc::new(WebSearch));
    registry.register(Arc::new(FetchUrl));
}

pub fn web_tools_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_web_tools(&mut registry);
    registry
}

fn http_client() -> Result<Client, ToolError> {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= max_redirects() {
                return attempt.stop();
            }
            match validate_url(attempt.url()) {
                Ok(()) => attempt.follow(),
                Err(_) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect to blocked URL",
                )),
            }
        }))
        .build()
        .map_err(|err| ToolError::Failed(format!("http client: {err}")))
}

fn http_client_same_host(origin_host: &str) -> Result<Client, ToolError> {
    let origin = normalize_host(origin_host);
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() >= max_redirects() {
                return attempt.stop();
            }
            let Some(host) = attempt.url().host_str() else {
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect to blocked URL",
                ));
            };
            if normalize_host(host) != origin {
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect to a different host",
                ));
            }
            match validate_url_allowing_private(attempt.url()) {
                Ok(()) => attempt.follow(),
                Err(_) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect to blocked URL",
                )),
            }
        }))
        .build()
        .map_err(|err| ToolError::Failed(format!("http client: {err}")))
}

fn required_string(input: &Value, key: &str) -> Result<String, ToolError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::Failed(format!("{key} is required")))
}

struct WebSearch;

impl Tool for WebSearch {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".into(),
            description: "Search the public web with Exa. Returns titles, URLs, and snippets."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of results (1-10, default 5)",
                        "minimum": 1,
                        "maximum": 10
                    }
                },
                "required": ["query"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let query = required_string(&input, "query")?;
        let count = input
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_COUNT)
            .clamp(1, MAX_COUNT);
        let Some(api_key) = context
            .search_api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        else {
            return Err(ToolError::Failed(MISSING_EXA_KEY.into()));
        };

        let client = http_client()?;
        let response = client
            .post(EXA_SEARCH_URL)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("x-api-key", api_key)
            .json(&json!({
                "query": query,
                "numResults": count,
                "type": "auto",
                "contents": {
                    "highlights": {
                        "maxCharacters": SNIPPET_MAX_CHARS
                    }
                }
            }))
            .send()
            .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))?;
        if !status.is_success() {
            return Err(ToolError::Failed(exa_status_message(status.as_u16())));
        }

        let text = format_exa_results(&body)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ExaResponse {
    #[serde(default)]
    results: Vec<ExaResult>,
}

#[derive(Debug, Deserialize)]
struct ExaResult {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    highlights: Option<Vec<String>>,
    #[serde(default)]
    summary: Option<String>,
}

/// Parse Exa Search JSON into model-facing text. Used by tests without network.
pub fn format_exa_results(body: &str) -> Result<String, ToolError> {
    let parsed: ExaResponse = serde_json::from_str(body)
        .map_err(|_| ToolError::Failed("couldn’t parse Exa Search response".into()))?;
    if parsed.results.is_empty() {
        return Ok("No results.".into());
    }
    let mut lines = Vec::with_capacity(parsed.results.len());
    for (index, item) in parsed.results.into_iter().enumerate() {
        let title = item.title.as_deref().unwrap_or("").trim();
        let url = item.url.as_deref().unwrap_or("").trim();
        let snippet = snippet_from_exa(&item);
        lines.push(format!(
            "{}. {}\n   {}\n   {}",
            index + 1,
            if title.is_empty() {
                "(no title)"
            } else {
                title
            },
            if url.is_empty() { "(no url)" } else { url },
            if snippet.is_empty() {
                "(no snippet)"
            } else {
                snippet.as_str()
            }
        ));
    }
    Ok(lines.join("\n\n"))
}

fn snippet_from_exa(item: &ExaResult) -> String {
    if let Some(highlights) = &item.highlights {
        let joined = highlights
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" … ");
        if !joined.is_empty() {
            return truncate_chars(&joined, SNIPPET_MAX_CHARS);
        }
    }
    if let Some(summary) = item
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return truncate_chars(summary, SNIPPET_MAX_CHARS);
    }
    if let Some(text) = item
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return truncate_chars(text, SNIPPET_MAX_CHARS);
    }
    String::new()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

fn exa_status_message(status: u16) -> String {
    match status {
        401 | 403 => "Exa rejected the API key.".into(),
        429 => "Exa rate limit reached. Try again later.".into(),
        _ => format!("Exa Search request failed (HTTP {status})."),
    }
}

fn fetch_failed_status(status: u16) -> String {
    format!("fetch failed (HTTP {status})")
}

fn auth_required_message(status: u16, challenge: Option<&str>) -> String {
    let scheme = challenge
        .map(summarize_challenge)
        .unwrap_or_else(|| "HTTP authentication".into());
    format!(
        "fetch failed (HTTP {status}). The host asked for {scheme}. Call fetch_url again with the same url and credential_ref from a Resource note. Crosspond will collect the login if it is not in Keychain. Do not use the browser, curl, wget, or run_command."
    )
}

fn authentication_failed_message() -> String {
    "authentication failed. The saved login was rejected.".into()
}

fn summarize_challenge(www: &str) -> String {
    if let Some(digest) = first_digest_challenge(www)
        && let Ok(prompt) = digest_auth::parse(digest)
    {
        let realm = prompt.realm.trim();
        if realm.is_empty() {
            return "Digest authentication".into();
        }
        return format!("Digest authentication (realm={realm})");
    }
    if www.to_ascii_lowercase().contains("basic") {
        return "Basic authentication".into();
    }
    "HTTP authentication".into()
}

fn first_digest_challenge(www: &str) -> Option<&str> {
    let lower = www.to_ascii_lowercase();
    let start = lower.find("digest")?;
    Some(www[start..].trim())
}

fn looks_like_basic(www: &str) -> bool {
    www.to_ascii_lowercase().contains("basic")
}

fn public_reqwest_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "request timed out".into()
    } else if err.is_connect() {
        "couldn’t connect".into()
    } else if err.is_redirect() {
        "redirect to a blocked URL".into()
    } else {
        "network request failed".into()
    }
}

struct FetchUrl;

impl Tool for FetchUrl {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fetch_url".into(),
            description: "Fetch an http(s) page as the Crosspond host (no browser cookies). Starts with an unauthenticated HEAD. If the host requires HTTP basic or digest auth, call again with credential_ref from a Resource note that lists this URL; Crosspond collects the login. Do not pass a username or password, and do not use curl or run_command.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute http or https URL"
                    },
                    "credential_ref": {
                        "type": "string",
                        "description": "credential_ref from a Resource note after fetch_url reported authentication required. Never pass a username or password."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let raw = required_string(&input, "url")?;
        let wants_login = optional_string(&input, "credential_ref").is_some();
        let url = if wants_login {
            validate_fetch_url_for_hosts(&raw, &context.credential_hosts)?
        } else {
            validate_fetch_url(&raw)?
        };
        let creds = if wants_login {
            match (
                context
                    .fill_username
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                context
                    .fill_password
                    .as_deref()
                    .filter(|value| !value.is_empty()),
            ) {
                (Some(user), Some(password)) => Some((user, password)),
                _ => {
                    return Err(ToolError::Failed(
                        "login was not provided; Crosspond must collect it from the user".into(),
                    ));
                }
            }
        } else {
            None
        };
        let client = if creds.is_some() {
            let origin = url.host_str().unwrap_or("");
            http_client_same_host(origin)?
        } else {
            http_client()?
        };
        fetch_page(&client, url, creds)
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let credential_ref =
            optional_string(input, "credential_ref").unwrap_or_else(|| "a saved login".into());
        let destination = context
            .credential_destination
            .clone()
            .or_else(|| {
                input
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(host_from_url)
            })
            .unwrap_or_else(|| "this host".into());
        (
            format!("Fetch {destination} with saved login {credential_ref}"),
            String::new(),
        )
    }

    fn target_host(&self, _context: &ToolContext, input: &Value) -> Option<String> {
        input
            .get("url")
            .and_then(Value::as_str)
            .and_then(host_from_url)
    }
}

fn optional_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn fetch_page(
    client: &Client,
    url: reqwest::Url,
    creds: Option<(&str, &str)>,
) -> Result<ToolResult, ToolError> {
    match creds {
        None => fetch_unauthenticated(client, url),
        Some((user, password)) => fetch_authenticated(client, url, user, password),
    }
}

const FETCH_UA: &str = "Crosspond/0.0.1 (+https://github.com/crcrworks/crosspond)";
const FETCH_ACCEPT: &str = "text/html, text/plain, */*";

fn send_request(
    client: &Client,
    method: reqwest::Method,
    url: reqwest::Url,
    authorization: Option<&str>,
) -> Result<reqwest::blocking::Response, ToolError> {
    let mut request: RequestBuilder = client
        .request(method, url)
        .header(reqwest::header::USER_AGENT, FETCH_UA);
    request = request.header(reqwest::header::ACCEPT, FETCH_ACCEPT);
    if let Some(value) = authorization {
        request = request.header(reqwest::header::AUTHORIZATION, value);
    }
    request
        .send()
        .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))
}

fn www_authenticate(response: &reqwest::blocking::Response) -> Option<String> {
    let values: Vec<&str> = response
        .headers()
        .get_all(reqwest::header::WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect();
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

fn content_type_of(response: &reqwest::blocking::Response) -> String {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn is_auth_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 401
}

fn is_method_not_allowed(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 405 | 501)
}

fn request_uri(url: &reqwest::Url) -> String {
    let path = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };
    match url.query() {
        Some(query) => format!("{path}?{query}"),
        None => path.to_string(),
    }
}

fn fetch_unauthenticated(client: &Client, url: reqwest::Url) -> Result<ToolResult, ToolError> {
    let head = send_request(client, reqwest::Method::HEAD, url.clone(), None)?;
    let head_status = head.status();
    let head_challenge = www_authenticate(&head);
    let _ = head.bytes();
    if is_auth_status(head_status) {
        return Err(ToolError::Failed(auth_required_message(
            head_status.as_u16(),
            head_challenge.as_deref(),
        )));
    }
    complete_get(client, url, None)
}

fn complete_get(
    client: &Client,
    url: reqwest::Url,
    authorization: Option<&str>,
) -> Result<ToolResult, ToolError> {
    let response = send_request(client, reqwest::Method::GET, url, authorization)?;
    let status = response.status();
    let challenge = www_authenticate(&response);
    let content_type = content_type_of(&response);
    let bytes = response
        .bytes()
        .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))?;
    if is_auth_status(status) && authorization.is_none() {
        return Err(ToolError::Failed(auth_required_message(
            status.as_u16(),
            challenge.as_deref(),
        )));
    }
    if !status.is_success() {
        return Err(ToolError::Failed(fetch_failed_status(status.as_u16())));
    }
    let text = decode_body(&bytes, &content_type);
    Ok(ToolResult {
        text: truncate_output(text),
        created_file: None,
        image: None,
    })
}

fn fetch_authenticated(
    client: &Client,
    url: reqwest::Url,
    user: &str,
    password: &str,
) -> Result<ToolResult, ToolError> {
    let head = send_request(client, reqwest::Method::HEAD, url.clone(), None)?;
    let mut status = head.status();
    let mut challenge = www_authenticate(&head);
    let _ = head.bytes();
    if is_method_not_allowed(status) || (!status.is_success() && !is_auth_status(status)) {
        let get = send_request(client, reqwest::Method::GET, url.clone(), None)?;
        status = get.status();
        challenge = www_authenticate(&get);
        let _ = get.bytes();
    }
    if status.is_success() {
        return complete_get(client, url, None);
    }
    if !is_auth_status(status) {
        return Err(ToolError::Failed(fetch_failed_status(status.as_u16())));
    }
    let Some(www) = challenge.as_deref() else {
        return Err(ToolError::Failed(authentication_failed_message()));
    };
    let authorization = authorization_header(www, user, password, &request_uri(&url))?;
    authenticated_get(client, url, user, password, &authorization)
}

fn authenticated_get(
    client: &Client,
    url: reqwest::Url,
    user: &str,
    password: &str,
    authorization: &str,
) -> Result<ToolResult, ToolError> {
    let response = send_request(
        client,
        reqwest::Method::GET,
        url.clone(),
        Some(authorization),
    )?;
    let status = response.status();
    let challenge = www_authenticate(&response);
    if status.as_u16() == 401
        && let Some(www) = challenge.as_deref()
        && www.to_ascii_lowercase().contains("stale=true")
    {
        let retry = authorization_header(www, user, password, &request_uri(&url))?;
        let retry_response = send_request(client, reqwest::Method::GET, url, Some(&retry))?;
        return finish_success_or_auth(retry_response);
    }
    finish_success_or_auth(response)
}

fn finish_success_or_auth(response: reqwest::blocking::Response) -> Result<ToolResult, ToolError> {
    let status = response.status();
    let content_type = content_type_of(&response);
    let bytes = response
        .bytes()
        .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))?;
    if status.as_u16() == 401 {
        return Err(ToolError::Failed(authentication_failed_message()));
    }
    if !status.is_success() {
        return Err(ToolError::Failed(fetch_failed_status(status.as_u16())));
    }
    let text = decode_body(&bytes, &content_type);
    Ok(ToolResult {
        text: truncate_output(text),
        created_file: None,
        image: None,
    })
}

fn authorization_header(
    www: &str,
    user: &str,
    password: &str,
    uri: &str,
) -> Result<String, ToolError> {
    if first_digest_challenge(www).is_some() {
        return digest_authorization(www, user, password, uri);
    }
    if looks_like_basic(www) {
        return Ok(basic_authorization(user, password));
    }
    Err(ToolError::Failed(
        "unsupported HTTP authentication. Call fetch_url with credential_ref; do not use curl."
            .into(),
    ))
}

fn digest_authorization(
    www: &str,
    user: &str,
    password: &str,
    uri: &str,
) -> Result<String, ToolError> {
    let digest = first_digest_challenge(www)
        .ok_or_else(|| ToolError::Failed("couldn't complete HTTP authentication".into()))?;
    let mut prompt = digest_auth::parse(digest)
        .map_err(|_| ToolError::Failed("couldn't complete HTTP authentication".into()))?;
    let context = digest_auth::AuthContext::new(user, password, uri);
    let answer = prompt
        .respond(&context)
        .map_err(|_| ToolError::Failed("couldn't complete HTTP authentication".into()))?;
    Ok(answer.to_string())
}

fn basic_authorization(user: &str, password: &str) -> String {
    use base64::Engine;
    let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
    format!("Basic {token}")
}

fn decode_body(bytes: &[u8], content_type: &str) -> String {
    let raw = String::from_utf8_lossy(bytes);
    let lower = content_type.to_ascii_lowercase();
    if lower.contains("html") || looks_like_html(&raw) {
        strip_html(&raw)
    } else {
        raw.into_owned()
    }
}

fn looks_like_html(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<HTML")
}

/// Minimal HTML → text: drop script/style blocks and tags; keep text nodes.
pub fn strip_html(html: &str) -> String {
    let without_scripts = drop_blocks(html, &["script", "style", "noscript"]);
    let mut out = String::with_capacity(without_scripts.len());
    let mut in_tag = false;
    let mut last_was_space = false;
    for ch in without_scripts.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ if in_tag => {}
            c if c.is_whitespace() => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            c => {
                out.push(c);
                last_was_space = false;
            }
        }
    }
    decode_basic_entities(out.trim())
}

fn drop_blocks(html: &str, tags: &[&str]) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        let mut skipped = false;
        for tag in tags {
            let open = format!("<{tag}");
            if lower[i..].starts_with(&open) {
                let next = lower.as_bytes().get(i + open.len()).copied();
                if matches!(next, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')) {
                    let close = format!("</{tag}>");
                    if let Some(rel) = lower[i..].find(&close) {
                        i += rel + close.len();
                        while i < html.len() && !html.is_char_boundary(i) {
                            i += 1;
                        }
                        skipped = true;
                        break;
                    }
                }
            }
        }
        if skipped {
            continue;
        }
        let ch = html[i..].chars().next().expect("char at boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn decode_basic_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_exa_results() {
        let body = r#"{
            "results": [
                {
                    "title": "Example",
                    "url": "https://example.com/",
                    "highlights": ["An example domain."]
                }
            ]
        }"#;
        let text = format_exa_results(body).unwrap();
        assert!(text.contains("Example"));
        assert!(text.contains("https://example.com/"));
        assert!(text.contains("An example domain."));
    }

    #[test]
    fn empty_exa_results() {
        assert_eq!(format_exa_results("{}").unwrap(), "No results.");
    }

    #[test]
    fn web_search_requires_key() {
        let context = ToolContext::new();
        let err = WebSearch
            .execute(&context, json!({"query": "rust async"}))
            .unwrap_err();
        assert!(err.to_string().contains("Exa API key"));
    }

    #[test]
    fn tool_context_debug_redacts_key() {
        let mut context = ToolContext::new();
        context.search_api_key = Some("exa_super_secret_token".into());
        let rendered = format!("{context:?}");
        assert!(rendered.contains("***"));
        assert!(!rendered.contains("exa_super_secret_token"));
        assert!(!rendered.contains("super_secret"));
    }

    #[test]
    fn strip_html_removes_tags_and_scripts() {
        let html = r#"<html><head><script>alert(1)</script><style>a{}</style></head>
            <body><h1>Hello</h1><p>World &amp; friends</p></body></html>"#;
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World & friends"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("<h1>"));
    }

    #[test]
    fn fetch_url_rejects_private() {
        let context = ToolContext::new();
        let err = FetchUrl
            .execute(&context, json!({"url": "http://127.0.0.1/"}))
            .unwrap_err();
        assert!(
            err.to_string().contains("private") || err.to_string().contains("blocked"),
            "{err}"
        );
    }

    #[test]
    fn fetch_auth_status_points_at_fetch_url_credential_ref() {
        let nonce = "dcd98b7102dd2f0e8b11d0f600bfb0c093";
        let www = format!(
            "Digest realm=\"Hello World!\", nonce=\"{nonce}\", algorithm=MD5, qop=\"auth\""
        );
        let unauthorized = auth_required_message(401, Some(&www));
        assert!(unauthorized.contains("HTTP 401"));
        assert!(unauthorized.contains("Digest authentication"));
        assert!(unauthorized.contains("Hello World!"));
        assert!(unauthorized.contains("credential_ref"));
        assert!(unauthorized.contains("fetch_url"));
        assert!(unauthorized.contains("browser"));
        assert!(unauthorized.contains("curl"));
        assert!(!unauthorized.contains(nonce));
        assert!(!unauthorized.contains("browser_navigate"));
        assert_eq!(fetch_failed_status(403), "fetch failed (HTTP 403)");
        assert!(!fetch_failed_status(403).contains("credential_ref"));
        assert_eq!(fetch_failed_status(404), "fetch failed (HTTP 404)");
    }

    #[test]
    fn fetch_url_with_credential_ref_requires_injected_login() {
        let mut context = ToolContext::new();
        context.credential_hosts = vec!["example.com".into()];
        let err = FetchUrl
            .execute(
                &context,
                json!({
                    "url": "https://example.com/",
                    "credential_ref": "lab.fileserver"
                }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("collect"));
        assert!(!err.to_string().contains("labuser"));
    }

    #[test]
    fn fetch_url_credential_ref_rejects_unbound_host() {
        let mut context = ToolContext::new();
        context.fill_username = Some("labuser".into());
        context.fill_password = Some("hunter2".into());
        context.credential_hosts = vec!["files.example.invalid".into()];
        let err = FetchUrl
            .execute(
                &context,
                json!({
                    "url": "https://example.com/",
                    "credential_ref": "lab.fileserver"
                }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("not for example.com"));
        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn fetch_url_credential_ref_requires_note_hosts() {
        let mut context = ToolContext::new();
        context.fill_username = Some("labuser".into());
        context.fill_password = Some("hunter2".into());
        let err = FetchUrl
            .execute(
                &context,
                json!({
                    "url": "https://example.com/",
                    "credential_ref": "lab.fileserver"
                }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("Resource"));
        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn fetch_url_digest_head_then_get_with_injected_login() {
        let server = DigestServer::spawn();
        let url = reqwest::Url::parse(&format!("http://{}/share/", server.addr)).unwrap();
        let client = http_client().unwrap();

        let denied = fetch_page(&client, url.clone(), None).unwrap_err();
        let message = denied.to_string();
        assert!(message.contains("HTTP 401"));
        assert!(message.contains("credential_ref"));
        assert!(message.contains("Digest"));
        assert!(!message.contains(TEST_PASSWORD));
        assert!(!message.contains(TEST_USER));
        assert!(!message.contains(&server.nonce));

        let listing = fetch_page(&client, url.clone(), Some((TEST_USER, TEST_PASSWORD))).unwrap();
        assert!(listing.text.contains("Index of /share"));
        assert!(listing.text.contains("Study"));
        assert!(!listing.text.contains(TEST_PASSWORD));

        let rejected = fetch_page(&client, url, Some((TEST_USER, "wrong-password"))).unwrap_err();
        let rejected = rejected.to_string();
        assert!(rejected.contains("authentication failed"));
        assert!(!rejected.contains(TEST_PASSWORD));
        assert!(!rejected.contains("wrong-password"));
        assert!(!rejected.contains(TEST_USER));
    }

    #[test]
    fn fetch_url_basic_auth_with_injected_login() {
        let server = BasicServer::spawn();
        let url = reqwest::Url::parse(&format!("http://{}/share/", server.addr)).unwrap();
        let client = http_client().unwrap();
        let denied = fetch_page(&client, url.clone(), None).unwrap_err();
        assert!(denied.to_string().contains("Basic authentication"));
        let listing = fetch_page(&client, url, Some((TEST_USER, TEST_PASSWORD))).unwrap();
        assert!(listing.text.contains("Index of /share"));
        assert!(!listing.text.contains(TEST_PASSWORD));
    }

    #[test]
    fn fetch_url_bound_loopback_digest_via_execute() {
        let server = DigestServer::spawn();
        let url = format!("http://{}/share/", server.addr);
        let mut context = ToolContext::new();
        context.fill_username = Some(TEST_USER.into());
        context.fill_password = Some(TEST_PASSWORD.into());
        context.credential_hosts = vec!["127.0.0.1".into()];
        let listing = FetchUrl
            .execute(
                &context,
                json!({
                    "url": url,
                    "credential_ref": "lab.fileserver"
                }),
            )
            .unwrap();
        assert!(listing.text.contains("Index of /share"));
        assert!(!listing.text.contains(TEST_PASSWORD));
    }

    #[test]
    fn fetch_url_forbidden_is_not_authentication_required() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::Write;
            for mut stream in listener.incoming().take(4).flatten() {
                let _ = stream
                    .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\n\r\nforbidden");
            }
        });
        let url = reqwest::Url::parse(&format!("http://{addr}/share/")).unwrap();
        let client = http_client().unwrap();
        let err = fetch_page(&client, url, None).unwrap_err().to_string();
        assert!(err.contains("HTTP 403"));
        assert!(!err.contains("credential_ref"));
    }

    const TEST_USER: &str = "alice";
    const TEST_PASSWORD: &str = "correct-horse";
    const DIGEST_NONCE: &str = "dcd98b7102dd2f0e8b11d0f600bfb0c093";
    const LISTING_BODY: &str = "<html><body>Index of /share Etc/ Study/</body></html>";

    struct DigestServer {
        addr: std::net::SocketAddr,
        nonce: String,
    }

    impl DigestServer {
        fn spawn() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                for stream in listener.incoming().take(16).flatten() {
                    handle_digest_conn(stream);
                }
            });
            Self {
                addr,
                nonce: DIGEST_NONCE.into(),
            }
        }
    }

    struct BasicServer {
        addr: std::net::SocketAddr,
    }

    impl BasicServer {
        fn spawn() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                for stream in listener.incoming().take(16).flatten() {
                    handle_basic_conn(stream);
                }
            });
            Self { addr }
        }
    }

    fn handle_digest_conn(mut stream: std::net::TcpStream) {
        let Ok((method, path, authorization)) = read_http_request(&mut stream) else {
            return;
        };
        let www = format!(
            "Digest realm=\"Hello World!\", nonce=\"{DIGEST_NONCE}\", algorithm=MD5, qop=\"auth\""
        );
        let authorized = authorization
            .as_deref()
            .is_some_and(|header| digest_matches(header, TEST_USER, TEST_PASSWORD));
        if method == "HEAD" {
            if authorized {
                write_response(&mut stream, 200, "text/html", b"");
            } else {
                write_auth_challenge(&mut stream, &www);
            }
            return;
        }
        if method == "GET" && path.starts_with("/share") {
            if authorized {
                write_response(&mut stream, 200, "text/html", LISTING_BODY.as_bytes());
            } else {
                write_auth_challenge(&mut stream, &www);
            }
        }
    }

    fn handle_basic_conn(mut stream: std::net::TcpStream) {
        let Ok((method, _, authorization)) = read_http_request(&mut stream) else {
            return;
        };
        let expected = basic_authorization(TEST_USER, TEST_PASSWORD);
        let authorized = authorization.as_deref() == Some(expected.as_str());
        if method == "HEAD" {
            if authorized {
                write_response(&mut stream, 200, "text/html", b"");
            } else {
                write_auth_challenge(&mut stream, "Basic realm=\"files\"");
            }
            return;
        }
        if authorized {
            write_response(&mut stream, 200, "text/html", LISTING_BODY.as_bytes());
        } else {
            write_auth_challenge(&mut stream, "Basic realm=\"files\"");
        }
    }

    fn digest_matches(header: &str, user: &str, password: &str) -> bool {
        let Ok(mut incoming) = digest_auth::AuthorizationHeader::parse(header) else {
            return false;
        };
        if incoming.username != user {
            return false;
        }
        let got = incoming.response.clone();
        let uri = incoming.uri.clone();
        let context = digest_auth::AuthContext::new(user, password, uri);
        incoming.digest(&context);
        incoming.response.eq_ignore_ascii_case(&got)
    }

    fn read_http_request(
        stream: &mut std::net::TcpStream,
    ) -> std::io::Result<(String, String, Option<String>)> {
        use std::io::Read;
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while buf.len() < 16 * 1024 {
            if stream.read(&mut byte)? == 0 {
                break;
            }
            buf.push(byte[0]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&buf);
        let mut lines = text.split("\r\n");
        let first = lines.next().unwrap_or("");
        let mut parts = first.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("/").to_string();
        let mut authorization = None;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_string());
            }
        }
        Ok((method, path, authorization))
    }

    fn write_auth_challenge(stream: &mut std::net::TcpStream, www: &str) {
        let _ = write_raw(
            stream,
            &format!(
                "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: {www}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
    }

    fn write_response(
        stream: &mut std::net::TcpStream,
        status: u16,
        content_type: &str,
        body: &[u8],
    ) {
        let reason = if status == 200 { "OK" } else { "Error" };
        let header = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = write_raw(stream, &header);
        let _ = std::io::Write::write_all(stream, body);
    }

    fn write_raw(stream: &mut std::net::TcpStream, text: &str) -> std::io::Result<()> {
        use std::io::Write;
        stream.write_all(text.as_bytes())
    }
}
