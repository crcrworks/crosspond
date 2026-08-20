//! Web search (Exa) and page fetch tools.
//!
//! Search provider is Exa for now; Settings / Keychain can grow a provider choice later.

use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::{Client, RequestBuilder};
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::registry::ToolRegistry;
use crate::ssrf::{max_redirects, validate_fetch_url, validate_url};
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

fn fetch_status_message(status: u16) -> String {
    match status {
        401 | 403 => format!(
            "fetch failed (HTTP {status}). If this page needs a login, open it with browser_navigate, then call fill_credential with only credential_ref from a Resource note. Do not use curl or run_command."
        ),
        _ => format!("fetch failed (HTTP {status})"),
    }
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
            description: "Fetch a public http(s) page and return its text content (HTML tags stripped when needed).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute http or https URL"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let raw = required_string(&input, "url")?;
        let url = validate_fetch_url(&raw)?;
        let client = http_client()?;
        let response = send_get(&client, url)?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = response
            .bytes()
            .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))?;
        if !status.is_success() {
            return Err(ToolError::Failed(fetch_status_message(status.as_u16())));
        }
        let text = decode_body(&bytes, &content_type);
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

fn send_get(client: &Client, url: reqwest::Url) -> Result<reqwest::blocking::Response, ToolError> {
    let request: RequestBuilder = client.get(url);
    request
        .header(
            reqwest::header::USER_AGENT,
            "Crosspond/0.0.1 (+https://github.com/crcrworks/crosspond)",
        )
        .header(reqwest::header::ACCEPT, "text/html, text/plain, */*")
        .send()
        .map_err(|err| ToolError::Failed(public_reqwest_error(&err)))
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
    fn fetch_auth_status_points_at_browser_fill_credential() {
        let unauthorized = fetch_status_message(401);
        assert!(unauthorized.contains("HTTP 401"));
        assert!(unauthorized.contains("browser_navigate"));
        assert!(unauthorized.contains("fill_credential"));
        assert!(unauthorized.contains("curl"));
        let forbidden = fetch_status_message(403);
        assert!(forbidden.contains("HTTP 403"));
        assert!(forbidden.contains("credential_ref"));
        assert_eq!(fetch_status_message(404), "fetch failed (HTTP 404)");
    }
}
