//! Chromium DOM tools. Implemented over a `BrowserBackend` (extension CDP).
//!
//! This crate must not depend on Tauri or Chrome APIs.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult, truncate_output};

use crate::browser_snapshot::parse_ref;

pub const EXTENSION_DISCONNECTED: &str = "Chrome extension is not connected. In Settings, load the unpacked Crosspond extension (chrome://extensions → Developer mode → Load unpacked → the extension/chrome folder). Until then, browser_* tools cannot run; do not fall back to Accessibility or screenshots for Chromium pages.";

/// JSON relay to the Chrome extension. Implemented in `crosspond-app`.
pub trait BrowserTransport: Send + Sync {
    fn is_connected(&self) -> bool;
    fn call(&self, request: Value) -> Result<Value, ToolError>;
}

/// Platform browser backend. Mocked in tests; live impl talks CDP through the extension.
pub trait BrowserBackend: Send + Sync {
    fn connected(&self) -> bool;
    fn current_host(&self) -> Option<String>;
    fn tabs(&self) -> Result<String, ToolError>;
    fn snapshot(&self) -> Result<String, ToolError>;
    fn text(&self) -> Result<String, ToolError>;
    fn navigate(&self, action: &str, url: Option<&str>) -> Result<String, ToolError>;
    fn click(&self, element_ref: &str) -> Result<String, ToolError>;
    fn type_text(&self, element_ref: &str, text: &str) -> Result<String, ToolError>;
    fn fill(&self, element_ref: &str, text: &str) -> Result<String, ToolError>;
    fn press_key(&self, key: &str) -> Result<String, ToolError>;
    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        element_ref: Option<&str>,
    ) -> Result<String, ToolError>;
    fn select_option(&self, element_ref: &str, value: &str) -> Result<String, ToolError>;
    fn new_tab(&self, url: Option<&str>) -> Result<String, ToolError>;
    fn describe_ref(&self, _element_ref: &str) -> Option<String> {
        None
    }
    /// Host/scheme/realm if Chromium is paused on HTTP auth. Never includes credentials.
    fn pending_http_auth(&self) -> Option<HttpAuthChallenge> {
        None
    }
    /// Continue a paused HTTP auth challenge. Values must never be logged.
    fn continue_http_auth(&self, username: &str, password: &str) -> Result<String, ToolError> {
        let _ = (username, password);
        Err(ToolError::Failed(
            "no HTTP authentication challenge is pending".into(),
        ))
    }
}

/// Public HTTP auth metadata. Never includes a username or password.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpAuthChallenge {
    pub host: String,
    pub scheme: String,
    pub realm: String,
}

/// Tool/model copy when Chromium pauses on basic/digest auth.
pub fn http_auth_required_message(host: &str, scheme: &str, realm: &str) -> String {
    let kind = {
        let scheme = scheme.trim();
        if scheme.is_empty() { "HTTP" } else { scheme }
    };
    let mut message = format!("{kind} authentication required for {host}.");
    let realm = realm.trim();
    if !realm.is_empty() {
        message.push_str(&format!(" Realm: {realm}."));
    }
    message.push_str(
        " Call fill_credential with credential_ref from the matching Resource note. Pass only credential_ref — no username, password, or Accessibility node ids. Do not use curl, run_command, or browser_fill.",
    );
    message
}

pub fn is_browser_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_tabs"
            | "browser_snapshot"
            | "browser_text"
            | "browser_navigate"
            | "browser_click"
            | "browser_type"
            | "browser_fill"
            | "browser_press_key"
            | "browser_scroll"
            | "browser_select"
            | "browser_new_tab"
    )
}

pub fn is_browser_write_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_navigate"
            | "browser_click"
            | "browser_type"
            | "browser_fill"
            | "browser_press_key"
            | "browser_scroll"
            | "browser_select"
            | "browser_new_tab"
    )
}

pub fn host_from_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let hostport = hostport
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(hostport);
    let host = if let Some(stripped) = hostport.strip_prefix('[') {
        stripped.split(']').next().unwrap_or(stripped)
    } else if let Some((name, port)) = hostport.rsplit_once(':')
        && port.chars().all(|ch| ch.is_ascii_digit())
    {
        name
    } else {
        hostport
    };
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() { None } else { Some(host) }
}

pub fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

pub fn site_is_blocked(blocked: &[String], host: &str) -> bool {
    let host = normalize_host(host);
    blocked.iter().any(|entry| normalize_host(entry) == host)
}

pub fn site_is_allowed(allowed: &[String], host: &str) -> bool {
    let host = normalize_host(host);
    allowed.iter().any(|entry| normalize_host(entry) == host)
}

/// One host per line (or URL). Drops blanks and comments.
pub fn parse_host_list<I, S>(lines: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = Vec::new();
    for line in lines {
        let line = line.as_ref().trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let host = if line.contains("://") {
            host_from_url(line)
        } else {
            let token = line
                .split(['/', '?', '#', ' ', '\t'])
                .next()
                .unwrap_or(line);
            let host = normalize_host(token);
            if host.is_empty() { None } else { Some(host) }
        };
        let Some(host) = host else {
            continue;
        };
        if !out.iter().any(|existing| existing == &host) {
            out.push(host);
        }
    }
    out
}

/// Always-disconnected backend used when the extension is not wired up.
pub struct DisconnectedBrowser;

impl BrowserBackend for DisconnectedBrowser {
    fn connected(&self) -> bool {
        false
    }

    fn current_host(&self) -> Option<String> {
        None
    }

    fn tabs(&self) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn snapshot(&self) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn text(&self) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn navigate(&self, _action: &str, _url: Option<&str>) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn click(&self, _element_ref: &str) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn type_text(&self, _element_ref: &str, _text: &str) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn fill(&self, _element_ref: &str, _text: &str) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn press_key(&self, _key: &str) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn scroll(
        &self,
        _direction: &str,
        _amount: u32,
        _element_ref: Option<&str>,
    ) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn select_option(&self, _element_ref: &str, _value: &str) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }

    fn new_tab(&self, _url: Option<&str>) -> Result<String, ToolError> {
        Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
    }
}

pub fn register_browser_tools(registry: &mut ToolRegistry, backend: Arc<dyn BrowserBackend>) {
    registry.register(Arc::new(BrowserTabs {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserSnapshot {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserText {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserNavigate {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserClick {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserType {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserFill {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserPressKey {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserScroll {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserSelect {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(BrowserNewTab { backend }));
}

fn ask_user_property() -> Value {
    json!({
        "type": "boolean",
        "description": "When UI approval is AI, true asks the user first and false runs immediately. Ignored in Auto and Manual modes. Prefer true when unsure."
    })
}

fn required_string(input: &Value, key: &str) -> Result<String, ToolError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed(format!("{key} is required")))
}

fn optional_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_element_ref(input: &Value) -> Result<String, ToolError> {
    let raw = input
        .get("ref")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    parse_ref(raw).map_err(ToolError::Failed)
}

fn ok_text(text: String) -> ToolResult {
    ToolResult {
        text: truncate_output(text),
        created_file: None,
        image: None,
    }
}

struct BrowserTabs {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserTabs {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_tabs".into(),
            description: "List open Chromium tabs (title, URL, active) through the Crosspond Chrome extension. Does not use Accessibility.".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }
    }

    fn execute(&self, _context: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
        Ok(ok_text(self.backend.tabs()?))
    }
}

struct BrowserSnapshot {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserSnapshot {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_snapshot".into(),
            description: "Read a compact accessibility outline of the active Chromium tab with refs such as a1f3-e2. Prefer this over get_accessibility_snapshot and take_screenshot for Chrome/Arc/Brave/Edge pages when the extension is connected. Refs are invalid after the next snapshot or navigation.".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn execute(&self, _context: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
        Ok(ok_text(self.backend.snapshot()?))
    }
}

struct BrowserText {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserText {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_text".into(),
            description: "Read visible plaintext from the active Chromium tab. Prefer browser_snapshot when you need to click or fill controls.".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn execute(&self, _context: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
        Ok(ok_text(self.backend.text()?))
    }
}

struct BrowserNavigate {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserNavigate {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_navigate".into(),
            description: "Navigate the active Chromium tab: goto, back, forward, or reload. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["goto", "back", "forward", "reload"],
                        "description": "Navigation action"
                    },
                    "url": {
                        "type": "string",
                        "description": "URL for goto"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["action"]
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, input: &Value) -> Option<String> {
        optional_string(input, "url")
            .as_deref()
            .and_then(host_from_url)
            .or_else(|| self.backend.current_host())
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let action = optional_string(input, "action").unwrap_or_else(|| "navigate".into());
        let host = self
            .target_host(_context, input)
            .unwrap_or_else(|| "this site".into());
        (format!("Browser {action} on {host}"), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let action = required_string(&input, "action")?;
        if !matches!(action.as_str(), "goto" | "back" | "forward" | "reload") {
            return Err(ToolError::Failed(
                "action must be goto, back, forward, or reload".into(),
            ));
        }
        let url = optional_string(&input, "url");
        if action == "goto" && url.is_none() {
            return Err(ToolError::Failed("url is required for goto".into()));
        }
        Ok(ok_text(self.backend.navigate(&action, url.as_deref())?))
    }
}

struct BrowserClick {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserClick {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_click".into(),
            description: "Click a control from the latest browser_snapshot by ref. Returns a fresh snapshot. Prefer this over ui_click for Chromium pages. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "Ref from the latest browser_snapshot, such as a1f3-e2"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["ref"]
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let id = parse_element_ref(input).unwrap_or_default();
        let label = self
            .backend
            .describe_ref(&id)
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "a control".into());
        let host = self
            .backend
            .current_host()
            .unwrap_or_else(|| "the page".into());
        (format!("Click \"{label}\" on {host}"), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let element_ref = parse_element_ref(&input)?;
        Ok(ok_text(self.backend.click(&element_ref)?))
    }
}

struct BrowserType {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserType {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_type".into(),
            description: "Type into a control from the latest browser_snapshot by ref without clearing it first. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" },
                    "text": { "type": "string" },
                    "ask_user": ask_user_property()
                },
                "required": ["ref", "text"]
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let id = parse_element_ref(input).unwrap_or_default();
        let label = self
            .backend
            .describe_ref(&id)
            .unwrap_or_else(|| "a field".into());
        (format!("Type into \"{label}\""), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let element_ref = parse_element_ref(&input)?;
        let text = required_string(&input, "text")?;
        Ok(ok_text(self.backend.type_text(&element_ref, &text)?))
    }
}

struct BrowserFill {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserFill {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_fill".into(),
            description: "Clear a field from the latest browser_snapshot and fill it. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" },
                    "text": { "type": "string" },
                    "ask_user": ask_user_property()
                },
                "required": ["ref", "text"]
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let id = parse_element_ref(input).unwrap_or_default();
        let label = self
            .backend
            .describe_ref(&id)
            .unwrap_or_else(|| "a field".into());
        (format!("Fill \"{label}\""), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let element_ref = parse_element_ref(&input)?;
        let text = required_string(&input, "text")?;
        Ok(ok_text(self.backend.fill(&element_ref, &text)?))
    }
}

struct BrowserPressKey {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserPressKey {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_press_key".into(),
            description: "Press a key in the active Chromium tab (Enter, Tab, Escape, or a shortcut like Meta+a). May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                    "ask_user": ask_user_property()
                },
                "required": ["key"]
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let key = optional_string(input, "key").unwrap_or_else(|| "a key".into());
        (format!("Press {key} in the browser"), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let key = required_string(&input, "key")?;
        Ok(ok_text(self.backend.press_key(&key)?))
    }
}

struct BrowserScroll {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserScroll {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_scroll".into(),
            description: "Scroll the active Chromium tab or a snapshot ref into view. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "left", "right"]
                    },
                    "amount": { "type": "integer", "minimum": 1 },
                    "ref": { "type": "string" },
                    "ask_user": ask_user_property()
                }
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let direction = optional_string(input, "direction").unwrap_or_else(|| "down".into());
        (format!("Scroll {direction} in the browser"), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let direction = optional_string(&input, "direction").unwrap_or_else(|| "down".into());
        if !matches!(direction.as_str(), "up" | "down" | "left" | "right") {
            return Err(ToolError::Failed(
                "direction must be up, down, left, or right".into(),
            ));
        }
        let amount = input
            .get("amount")
            .and_then(Value::as_u64)
            .unwrap_or(3)
            .clamp(1, 50) as u32;
        let element_ref = optional_string(&input, "ref")
            .map(|raw| parse_ref(&raw).map_err(ToolError::Failed))
            .transpose()?;
        Ok(ok_text(self.backend.scroll(
            &direction,
            amount,
            element_ref.as_deref(),
        )?))
    }
}

struct BrowserSelect {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserSelect {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_select".into(),
            description: "Choose an option in a select control from the latest browser_snapshot. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" },
                    "value": { "type": "string" },
                    "ask_user": ask_user_property()
                },
                "required": ["ref", "value"]
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, _input: &Value) -> Option<String> {
        self.backend.current_host()
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let id = parse_element_ref(input).unwrap_or_default();
        let label = self
            .backend
            .describe_ref(&id)
            .unwrap_or_else(|| "a select".into());
        (format!("Choose an option in \"{label}\""), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let element_ref = parse_element_ref(&input)?;
        let value = required_string(&input, "value")?;
        Ok(ok_text(self.backend.select_option(&element_ref, &value)?))
    }
}

struct BrowserNewTab {
    backend: Arc<dyn BrowserBackend>,
}

impl Tool for BrowserNewTab {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_new_tab".into(),
            description:
                "Open a new Chromium tab in a Crosspond tab group. May require user approval."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "ask_user": ask_user_property()
                }
            }),
        }
    }

    fn target_host(&self, _context: &ToolContext, input: &Value) -> Option<String> {
        optional_string(input, "url")
            .as_deref()
            .and_then(host_from_url)
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let host = self
            .target_host(_context, input)
            .unwrap_or_else(|| "a new tab".into());
        (format!("Open {host} in Chrome"), String::new())
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let url = optional_string(&input, "url");
        Ok(ok_text(self.backend.new_tab(url.as_deref())?))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::calendar::MockCalendar;
    use crate::computer::{
        AccessibilityBackend, AppBackend, InputBackend, Screenshot, ScreenshotBackend,
        computer_and_screenshot_registry_with_browser,
    };
    use serde_json::json;
    use std::sync::Mutex;

    pub(crate) struct MockBrowser {
        pub connected: bool,
        pub host: Mutex<Option<String>>,
        pub snapshot: String,
        pub clicks: Mutex<Vec<String>>,
        pub fills: Mutex<Vec<(String, String)>>,
        pub pending_auth: Mutex<Option<HttpAuthChallenge>>,
        pub http_auth_fills: Mutex<Vec<(String, String)>>,
    }

    impl MockBrowser {
        pub(crate) fn connected_page() -> Self {
            Self {
                connected: true,
                host: Mutex::new(Some("example.com".into())),
                snapshot:
                    "Page: Example\nURL: https://example.com/\n\nbutton \"Continue\" [a1f3-e1]\n"
                        .into(),
                clicks: Mutex::new(Vec::new()),
                fills: Mutex::new(Vec::new()),
                pending_auth: Mutex::new(None),
                http_auth_fills: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn with_digest_auth() -> Self {
            let page = Self::connected_page();
            *page.host.lock().expect("host") = Some("files.example.invalid".into());
            *page.pending_auth.lock().expect("auth") = Some(HttpAuthChallenge {
                host: "files.example.invalid".into(),
                scheme: "digest".into(),
                realm: "lab-share".into(),
            });
            page
        }
    }

    impl BrowserBackend for MockBrowser {
        fn connected(&self) -> bool {
            self.connected
        }

        fn current_host(&self) -> Option<String> {
            self.host.lock().expect("host").clone()
        }

        fn tabs(&self) -> Result<String, ToolError> {
            Ok("1. Example — https://example.com/ (active)".into())
        }

        fn snapshot(&self) -> Result<String, ToolError> {
            Ok(self.snapshot.clone())
        }

        fn text(&self) -> Result<String, ToolError> {
            Ok("Example Domain".into())
        }

        fn navigate(&self, action: &str, url: Option<&str>) -> Result<String, ToolError> {
            if let Some(url) = url
                && let Some(host) = host_from_url(url)
            {
                *self.host.lock().expect("host") = Some(host);
            }
            Ok(format!("Navigated {action}\n\n{}", self.snapshot))
        }

        fn click(&self, element_ref: &str) -> Result<String, ToolError> {
            if !self.snapshot.contains(element_ref) {
                return Err(ToolError::Failed(
                    "stale or unknown ref. Call browser_snapshot again.".into(),
                ));
            }
            self.clicks
                .lock()
                .expect("clicks")
                .push(element_ref.to_string());
            Ok(format!("Clicked {element_ref}.\n\n{}", self.snapshot))
        }

        fn type_text(&self, element_ref: &str, text: &str) -> Result<String, ToolError> {
            self.fills
                .lock()
                .expect("fills")
                .push((element_ref.into(), text.into()));
            Ok(format!("Typed into {element_ref}.\n\n{}", self.snapshot))
        }

        fn fill(&self, element_ref: &str, text: &str) -> Result<String, ToolError> {
            self.fills
                .lock()
                .expect("fills")
                .push((element_ref.into(), text.into()));
            Ok(format!("Filled {element_ref}.\n\n{}", self.snapshot))
        }

        fn press_key(&self, key: &str) -> Result<String, ToolError> {
            Ok(format!("Pressed {key}.\n\n{}", self.snapshot))
        }

        fn scroll(
            &self,
            direction: &str,
            amount: u32,
            _element_ref: Option<&str>,
        ) -> Result<String, ToolError> {
            Ok(format!(
                "Scrolled {direction} {amount}.\n\n{}",
                self.snapshot
            ))
        }

        fn select_option(&self, element_ref: &str, value: &str) -> Result<String, ToolError> {
            Ok(format!(
                "Selected {value} in {element_ref}.\n\n{}",
                self.snapshot
            ))
        }

        fn new_tab(&self, url: Option<&str>) -> Result<String, ToolError> {
            Ok(format!(
                "Opened tab {}\n\n{}",
                url.unwrap_or("about:blank"),
                self.snapshot
            ))
        }

        fn describe_ref(&self, element_ref: &str) -> Option<String> {
            if element_ref == "a1f3-e1" {
                Some("Continue".into())
            } else {
                None
            }
        }

        fn pending_http_auth(&self) -> Option<HttpAuthChallenge> {
            self.pending_auth.lock().expect("auth").clone()
        }

        fn continue_http_auth(&self, username: &str, password: &str) -> Result<String, ToolError> {
            if self.pending_auth.lock().expect("auth").is_none() {
                return Err(ToolError::Failed(
                    "no HTTP authentication challenge is pending".into(),
                ));
            }
            self.http_auth_fills
                .lock()
                .expect("fills")
                .push((username.to_string(), password.to_string()));
            *self.pending_auth.lock().expect("auth") = None;
            Ok("Filled HTTP authentication. Values were not returned.".into())
        }
    }

    struct PanicShot;

    impl ScreenshotBackend for PanicShot {
        fn capture(
            &self,
            _pid: Option<i32>,
            _app_name: Option<&str>,
        ) -> Result<Screenshot, ToolError> {
            panic!("screenshot backend must not run for browser_* tools");
        }

        fn click(&self, _x: u32, _y: u32) -> Result<String, ToolError> {
            panic!("screenshot backend must not run for browser_* tools");
        }

        fn recapture(&self) -> Result<Screenshot, ToolError> {
            panic!("screenshot backend must not run for browser_* tools");
        }
    }

    struct NoopAx;

    impl AccessibilityBackend for NoopAx {
        fn snapshot(
            &self,
            _pid: Option<i32>,
            _app_name: Option<&str>,
        ) -> Result<String, ToolError> {
            panic!("accessibility backend must not run for browser_* tools");
        }

        fn press(&self, _node_id: &str) -> Result<String, ToolError> {
            panic!("accessibility backend must not run for browser_* tools");
        }

        fn set_value(&self, _node_id: &str, _value: &str) -> Result<String, ToolError> {
            panic!("accessibility backend must not run for browser_* tools");
        }

        fn describe_node(&self, _node_id: &str) -> Option<String> {
            None
        }
    }

    struct NoopApps;

    impl AppBackend for NoopApps {
        fn list_apps(&self) -> Result<String, ToolError> {
            Ok(String::new())
        }

        fn open_app(
            &self,
            _name: Option<&str>,
            _bundle_id: Option<&str>,
        ) -> Result<String, ToolError> {
            Ok(String::new())
        }

        fn focus_app(
            &self,
            _name: Option<&str>,
            _bundle_id: Option<&str>,
        ) -> Result<String, ToolError> {
            Ok(String::new())
        }

        fn resolve_running_app(&self, app: &str) -> Result<(i32, String), ToolError> {
            Err(ToolError::Failed(app.into()))
        }
    }

    struct NoopInput;

    impl InputBackend for NoopInput {
        fn type_text(&self, _text: &str, _node_id: Option<&str>) -> Result<String, ToolError> {
            Ok(String::new())
        }

        fn hotkey(&self, _keys: &[String]) -> Result<String, ToolError> {
            Ok(String::new())
        }

        fn scroll(
            &self,
            _direction: &str,
            _amount: u32,
            _by: &str,
            _node_id: Option<&str>,
            _x: Option<u32>,
            _y: Option<u32>,
        ) -> Result<String, ToolError> {
            Ok(String::new())
        }
    }

    fn registry(browser: Arc<dyn BrowserBackend>) -> ToolRegistry {
        computer_and_screenshot_registry_with_browser(
            Arc::new(NoopAx),
            Arc::new(PanicShot),
            Arc::new(NoopApps),
            Arc::new(NoopInput),
            Arc::new(MockCalendar),
            browser,
        )
    }

    #[test]
    fn disconnected_browser_explains_install() {
        let err = registry(Arc::new(DisconnectedBrowser))
            .execute("browser_snapshot", &ToolContext::new(), json!({}))
            .unwrap_err();
        assert!(err.to_string().contains("Load unpacked"));
        assert!(err.to_string().contains("do not fall back"));
    }

    #[test]
    fn snapshot_then_click_by_ref_skips_screenshot() {
        let browser = Arc::new(MockBrowser::connected_page());
        let tools = registry(Arc::clone(&browser) as Arc<dyn BrowserBackend>);
        let snap = tools
            .execute("browser_snapshot", &ToolContext::new(), json!({}))
            .unwrap();
        assert!(snap.text.contains("a1f3-e1"));
        let clicked = tools
            .execute(
                "browser_click",
                &ToolContext::new(),
                json!({"ref": "a1f3-e1"}),
            )
            .unwrap();
        assert!(clicked.text.contains("Clicked a1f3-e1"));
        assert_eq!(browser.clicks.lock().unwrap().as_slice(), ["a1f3-e1"]);
    }

    #[test]
    fn fill_does_not_echo_value_in_approval_copy() {
        let browser = Arc::new(MockBrowser::connected_page());
        let tools = registry(browser);
        let (title, body) = tools.approval_prompt(
            "browser_fill",
            &ToolContext::new(),
            &json!({"ref": "a1f3-e1", "text": "hunter2"}),
        );
        assert!(!title.contains("hunter2"));
        assert!(!body.contains("hunter2"));
    }

    #[test]
    fn http_auth_required_message_points_at_fill_credential() {
        let text = http_auth_required_message("files.example.invalid", "digest", "lab-share");
        assert!(text.contains("digest authentication required"));
        assert!(text.contains("files.example.invalid"));
        assert!(text.contains("fill_credential"));
        assert!(text.contains("only credential_ref"));
        assert!(text.contains("curl"));
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("labuser"));
    }

    #[test]
    fn host_from_url_strips_port_and_path() {
        assert_eq!(
            host_from_url("https://Mail.Example.com:443/inbox?q=1"),
            Some("mail.example.com".into())
        );
        assert_eq!(host_from_url("about:blank"), None);
        assert_eq!(host_from_url("chrome://extensions"), None);
    }

    #[test]
    fn blocked_and_allowed_hosts_match_normalized() {
        assert!(site_is_blocked(&["Example.COM".into()], "example.com"));
        assert!(site_is_allowed(&["example.com".into()], "EXAMPLE.com."));
        assert!(!site_is_allowed(&["other.com".into()], "example.com"));
    }

    #[test]
    fn parse_host_list_accepts_urls_and_skips_comments() {
        let hosts = parse_host_list([
            "Example.COM",
            "https://mail.example.com/inbox",
            "# ignore",
            "",
            "example.com",
        ]);
        assert_eq!(
            hosts,
            vec!["example.com".to_string(), "mail.example.com".to_string()]
        );
    }
}
