use std::sync::Arc;

use serde_json::{Value, json};

use crate::ax_outline::truncate_ax_text;
use crate::browser::{BrowserBackend, DisconnectedBrowser};
use crate::registry::ToolRegistry;
use crate::tool::{
    Tool, ToolContext, ToolDefinition, ToolError, ToolImage, ToolResult, truncate_output,
};

/// Platform Accessibility backend. Implemented in `crosspond-macos`.
///
/// This crate must not depend on macOS frameworks.
pub trait AccessibilityBackend: Send + Sync {
    fn snapshot(&self, pid: Option<i32>, app_name: Option<&str>) -> Result<String, ToolError>;
    fn press(&self, node_id: &str) -> Result<String, ToolError>;
    fn set_value(&self, node_id: &str, value: &str) -> Result<String, ToolError>;
    /// Fill a password field. Host-only; the model never supplies `value`.
    fn set_secure_value(&self, node_id: &str, value: &str) -> Result<String, ToolError> {
        let _ = (node_id, value);
        Err(ToolError::Failed("secure fill is not available".into()))
    }
    fn describe_node(&self, node_id: &str) -> Option<String>;
    fn is_secure_node(&self, _node_id: &str) -> bool {
        false
    }
    fn focused_is_secure(&self) -> bool {
        false
    }
}

/// Running and installed Mac apps. Implemented in `crosspond-macos`.
pub trait AppBackend: Send + Sync {
    fn list_apps(&self) -> Result<String, ToolError>;
    fn open_app(&self, name: Option<&str>, bundle_id: Option<&str>) -> Result<String, ToolError>;
    fn focus_app(&self, name: Option<&str>, bundle_id: Option<&str>) -> Result<String, ToolError>;
    fn resolve_running_app(&self, app: &str) -> Result<(i32, String), ToolError>;
}

/// Keyboard and scroll input. Implemented in `crosspond-macos`.
pub trait InputBackend: Send + Sync {
    fn type_text(&self, text: &str, node_id: Option<&str>) -> Result<String, ToolError>;
    fn hotkey(&self, keys: &[String]) -> Result<String, ToolError>;
    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        by: &str,
        node_id: Option<&str>,
        x: Option<u32>,
        y: Option<u32>,
    ) -> Result<String, ToolError>;
}

/// Window screenshot and image-coordinate click. Implemented in `crosspond-macos`.
pub trait ScreenshotBackend: Send + Sync {
    fn capture(&self, pid: Option<i32>, app_name: Option<&str>) -> Result<Screenshot, ToolError>;
    fn click(&self, x: u32, y: u32) -> Result<String, ToolError>;
    /// Re-capture the window last used for screenshot/click, not the ambient app.
    fn recapture(&self) -> Result<Screenshot, ToolError>;
}

/// Captured window image for the model.
///
/// `Debug` redacts image bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct Screenshot {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub app_name: String,
}

impl std::fmt::Debug for Screenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screenshot")
            .field("bytes_len", &self.bytes.len())
            .field("media_type", &self.media_type)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("app_name", &self.app_name)
            .finish()
    }
}

pub fn register_computer_tools(
    registry: &mut ToolRegistry,
    backend: Arc<dyn AccessibilityBackend>,
    apps: Arc<dyn AppBackend>,
    browser: Arc<dyn BrowserBackend>,
) {
    registry.register(Arc::new(GetAccessibilitySnapshot {
        backend: Arc::clone(&backend),
        apps: Arc::clone(&apps),
    }));
    registry.register(Arc::new(UiPress {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(UiSetValue {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(FillCredential {
        ax: backend,
        browser,
    }));
}

pub fn register_screenshot_tools(
    registry: &mut ToolRegistry,
    backend: Arc<dyn ScreenshotBackend>,
    apps: Arc<dyn AppBackend>,
) {
    registry.register(Arc::new(TakeScreenshot {
        backend: Arc::clone(&backend),
        apps: Arc::clone(&apps),
    }));
    registry.register(Arc::new(UiClick { backend }));
}

pub fn register_app_tools(registry: &mut ToolRegistry, apps: Arc<dyn AppBackend>) {
    registry.register(Arc::new(ListApps {
        backend: Arc::clone(&apps),
    }));
    registry.register(Arc::new(OpenApp {
        backend: Arc::clone(&apps),
    }));
    registry.register(Arc::new(FocusApp { backend: apps }));
}

pub fn register_input_tools(
    registry: &mut ToolRegistry,
    input: Arc<dyn InputBackend>,
    ax: Arc<dyn AccessibilityBackend>,
) {
    registry.register(Arc::new(UiType {
        backend: Arc::clone(&input),
        ax,
    }));
    registry.register(Arc::new(UiHotkey {
        backend: Arc::clone(&input),
    }));
    registry.register(Arc::new(UiScroll { backend: input }));
}

pub fn computer_registry(
    backend: Arc<dyn AccessibilityBackend>,
    apps: Arc<dyn AppBackend>,
) -> ToolRegistry {
    let mut registry = crate::filesystem_registry();
    register_computer_tools(&mut registry, backend, apps, Arc::new(DisconnectedBrowser));
    registry
}

pub fn computer_and_screenshot_registry(
    ax: Arc<dyn AccessibilityBackend>,
    screenshot: Arc<dyn ScreenshotBackend>,
    apps: Arc<dyn AppBackend>,
    input: Arc<dyn InputBackend>,
    calendar: Arc<dyn crate::calendar::CalendarBackend>,
) -> ToolRegistry {
    computer_and_screenshot_registry_with_browser(
        ax,
        screenshot,
        apps,
        input,
        calendar,
        Arc::new(crate::browser::DisconnectedBrowser),
    )
}

pub fn computer_and_screenshot_registry_with_browser(
    ax: Arc<dyn AccessibilityBackend>,
    screenshot: Arc<dyn ScreenshotBackend>,
    apps: Arc<dyn AppBackend>,
    input: Arc<dyn InputBackend>,
    calendar: Arc<dyn crate::calendar::CalendarBackend>,
    browser: Arc<dyn BrowserBackend>,
) -> ToolRegistry {
    let mut registry = crate::filesystem_registry();
    register_computer_tools(
        &mut registry,
        Arc::clone(&ax),
        Arc::clone(&apps),
        Arc::clone(&browser),
    );
    register_screenshot_tools(&mut registry, screenshot, Arc::clone(&apps));
    register_app_tools(&mut registry, apps);
    register_input_tools(&mut registry, input, ax);
    crate::calendar::register_calendar_tools(&mut registry, calendar);
    crate::web::register_web_tools(&mut registry);
    crate::shell::register_shell_tools(&mut registry);
    crate::browser::register_browser_tools(&mut registry, browser);
    registry
}

fn ask_user_property() -> Value {
    json!({
        "type": "boolean",
        "description": "When UI approval is AI, true asks the user first and false runs immediately. Ignored in Auto and Manual modes. Prefer true when unsure."
    })
}

fn app_property() -> Value {
    json!({
        "type": "string",
        "description": "Optional running app name or bundle id to target instead of the ambient frontmost app"
    })
}

fn optional_app(input: &Value) -> Option<String> {
    input
        .get("app")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn optional_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn resolve_target(
    context: &ToolContext,
    input: &Value,
    apps: &dyn AppBackend,
) -> Result<(Option<i32>, Option<String>), ToolError> {
    if let Some(app) = optional_app(input) {
        let (pid, name) = apps.resolve_running_app(&app)?;
        Ok((Some(pid), Some(name)))
    } else {
        Ok((context.frontmost_pid, context.frontmost_name.clone()))
    }
}

fn parse_named_node_id(input: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        _ => Err(ToolError::Failed(format!("{key} must be a node id"))),
    }
}

fn parse_node_id(input: &Value) -> Result<String, ToolError> {
    match input.get("node_id") {
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(Value::Number(value)) => Ok(value.to_string()),
        _ => Err(ToolError::Failed("node_id is required".into())),
    }
}

fn parse_u32(input: &Value, key: &str) -> Result<u32, ToolError> {
    match input.get(key) {
        Some(Value::Number(value)) => {
            if let Some(n) = value.as_u64() {
                return u32::try_from(n)
                    .map_err(|_| ToolError::Failed(format!("{key} is out of range")));
            }
            if let Some(n) = value.as_f64()
                && n.is_finite()
                && n >= 0.0
                && n <= f64::from(u32::MAX)
            {
                return Ok(n.round() as u32);
            }
            Err(ToolError::Failed(format!(
                "{key} must be a non-negative number"
            )))
        }
        Some(Value::String(value)) => {
            if let Ok(n) = value.parse::<u32>() {
                return Ok(n);
            }
            value
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0 && *n <= f64::from(u32::MAX))
                .map(|n| n.round() as u32)
                .ok_or_else(|| ToolError::Failed(format!("{key} must be a non-negative number")))
        }
        _ => Err(ToolError::Failed(format!("{key} is required"))),
    }
}

fn optional_u32(input: &Value, key: &str) -> Option<u32> {
    input.get(key).and_then(|value| {
        if let Some(n) = value.as_u64() {
            u32::try_from(n).ok()
        } else if let Some(n) = value.as_f64()
            && n.is_finite()
            && n >= 0.0
            && n <= f64::from(u32::MAX)
        {
            Some(n.round() as u32)
        } else if let Value::String(s) = value {
            s.parse::<u32>().ok().or_else(|| {
                s.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && *n >= 0.0 && *n <= f64::from(u32::MAX))
                    .map(|n| n.round() as u32)
            })
        } else {
            None
        }
    })
}

fn parse_app_identifiers(input: &Value) -> Result<(Option<String>, Option<String>), ToolError> {
    let name = optional_string(input, "name");
    let bundle_id = optional_string(input, "bundle_id");
    if name.is_none() && bundle_id.is_none() {
        return Err(ToolError::Failed("name or bundle_id is required".into()));
    }
    Ok((name, bundle_id))
}

fn app_display_name(name: Option<&str>, bundle_id: Option<&str>) -> String {
    name.filter(|s| !s.is_empty())
        .or(bundle_id.filter(|s| !s.is_empty()))
        .unwrap_or("app")
        .to_string()
}

fn app_clause(context: &ToolContext) -> String {
    match context.frontmost_name.as_deref() {
        Some(name) if !name.is_empty() => format!("in {name}"),
        _ => "in the frontmost app".into(),
    }
}

fn parse_scroll_direction(input: &Value) -> Result<String, ToolError> {
    let direction = input
        .get("direction")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    match direction {
        "up" | "down" | "left" | "right" => Ok(direction.to_string()),
        "" => Err(ToolError::Failed("direction is required".into())),
        _ => Err(ToolError::Failed(
            "direction must be up, down, left, or right".into(),
        )),
    }
}

fn parse_scroll_by(input: &Value) -> String {
    match input
        .get("by")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some("page") => "page".into(),
        _ => "line".into(),
    }
}

fn parse_hotkey_keys(input: &Value) -> Result<Vec<String>, ToolError> {
    let keys = input
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Failed("keys is required".into()))?;
    if keys.len() < 2 {
        return Err(ToolError::Failed(
            "keys must contain at least two entries".into(),
        ));
    }
    let mut parsed = Vec::with_capacity(keys.len());
    for key in keys {
        let value = key
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::Failed("each key must be a non-empty string".into()))?;
        parsed.push(value.to_string());
    }
    Ok(parsed)
}

struct GetAccessibilitySnapshot {
    backend: Arc<dyn AccessibilityBackend>,
    apps: Arc<dyn AppBackend>,
}

impl Tool for GetAccessibilitySnapshot {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_accessibility_snapshot".into(),
            description: "Read a compact Accessibility tree of the app the user was in when they opened Crosspond, or an optional running app. Prefer this before screenshots when the target has a visible name (buttons, channels, tabs). Node ids are only valid until the next snapshot or UI action.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "app": app_property()
                }
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let (pid, name) = resolve_target(context, &input, self.apps.as_ref())?;
        let text = self.backend.snapshot(pid, name.as_deref())?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct UiPress {
    backend: Arc<dyn AccessibilityBackend>,
}

impl Tool for UiPress {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ui_press".into(),
            description: "Activate a control from the latest accessibility snapshot by node id. Clicks the labeled control (more reliable than ui_click visual guesses). Prefer this for labeled UI such as Discord channels. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "description": "Node id from the latest get_accessibility_snapshot (string or number)"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["node_id"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let id = parse_node_id(input).unwrap_or_default();
        let label = self
            .backend
            .describe_node(&id)
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "a control".into());
        (format!("Press \"{label}\""), app_clause(context))
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let node_id = parse_node_id(&input)?;
        let text = self.backend.press(&node_id)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct UiSetValue {
    backend: Arc<dyn AccessibilityBackend>,
}

impl Tool for UiSetValue {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ui_set_value".into(),
            description: "Set the value of a text field from the latest accessibility snapshot. Do not use this for passwords; call fill_credential. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "description": "Node id from the latest get_accessibility_snapshot (string or number)"
                    },
                    "value": {
                        "type": "string",
                        "description": "Text to put in the field"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["node_id", "value"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let id = parse_node_id(input).unwrap_or_default();
        let label = self
            .backend
            .describe_node(&id)
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "a field".into());
        let title = if self.backend.is_secure_node(&id) {
            format!("Fill \"{label}\"")
        } else {
            let value = input
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("Set \"{label}\" to \"{}\"", truncate_ax_text(value))
        };
        (title, app_clause(context))
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let node_id = parse_node_id(&input)?;
        let value = input
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("value is required".into()))?;
        let text = self.backend.set_value(&node_id, value)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct FillCredential {
    ax: Arc<dyn AccessibilityBackend>,
    browser: Arc<dyn BrowserBackend>,
}

impl Tool for FillCredential {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fill_credential".into(),
            description: "Fill a login from a Knowledge Vault credential_ref. Native dialogs: pass username_node_id and/or password_node_id from the latest get_accessibility_snapshot. Chromium HTTP basic/digest auth: omit node ids after browser_navigate reports authentication required. Never pass a username or password; Crosspond collects those from the user or Keychain.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "credential_ref": {
                        "type": "string",
                        "description": "credential_ref from a Resource note. Do not invent a new name."
                    },
                    "username_node_id": {
                        "description": "Username field node id from the latest get_accessibility_snapshot. Omit for Chromium HTTP authentication."
                    },
                    "password_node_id": {
                        "description": "Password field node id from the latest get_accessibility_snapshot. Omit for Chromium HTTP authentication."
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["credential_ref"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let credential_ref = input
            .get("credential_ref")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("a saved login");
        (
            format!("Fill saved login for {credential_ref}"),
            app_clause(context),
        )
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let credential_ref = input
            .get("credential_ref")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::Failed("credential_ref is required".into()))?;
        let username_node = parse_named_node_id(&input, "username_node_id")?;
        let password_node = parse_named_node_id(&input, "password_node_id")?;
        let username = context
            .fill_username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let password = context
            .fill_password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if username_node.is_none() && password_node.is_none() {
            let Some(challenge) = self.browser.pending_http_auth() else {
                return Err(ToolError::Failed(
                    "no HTTP authentication challenge is pending. For Chromium basic/digest auth, call browser_navigate or browser_new_tab first, then fill_credential with only credential_ref. For native login dialogs, pass username_node_id and/or password_node_id from get_accessibility_snapshot.".into(),
                ));
            };
            let Some(username) = username else {
                return Err(ToolError::Failed(
                    "login was not provided; Crosspond must collect it from the user".into(),
                ));
            };
            let Some(password) = password else {
                return Err(ToolError::Failed(
                    "login was not provided; Crosspond must collect it from the user".into(),
                ));
            };
            self.browser.continue_http_auth(username, password)?;
            return Ok(ToolResult {
                text: format!(
                    "Filled login for {credential_ref} on {}. Values were not returned.",
                    challenge.host
                ),
                created_file: None,
                image: None,
            });
        }
        if username_node.is_some() && username.is_none() {
            return Err(ToolError::Failed(
                "login was not provided; Crosspond must collect it from the user".into(),
            ));
        }
        if password_node.is_some() && password.is_none() {
            return Err(ToolError::Failed(
                "login was not provided; Crosspond must collect it from the user".into(),
            ));
        }
        if let (Some(node_id), Some(value)) = (username_node.as_deref(), username) {
            if self.ax.is_secure_node(node_id) {
                self.ax.set_secure_value(node_id, value)?;
            } else {
                self.ax.set_value(node_id, value)?;
            }
        }
        if let (Some(node_id), Some(value)) = (password_node.as_deref(), password) {
            self.ax.set_secure_value(node_id, value)?;
        }
        Ok(ToolResult {
            text: format!("Filled login for {credential_ref}. Values were not returned."),
            created_file: None,
            image: None,
        })
    }
}

struct TakeScreenshot {
    backend: Arc<dyn ScreenshotBackend>,
    apps: Arc<dyn AppBackend>,
}

impl Tool for TakeScreenshot {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "take_screenshot".into(),
            description: "Capture a screenshot of the window of the app the user was in when they opened Crosspond, or an optional running app. Use only when Accessibility has no useful label for the target (canvas, unlabeled icons, some web pages). ui_click uses exact pixel coordinates in this image (origin top-left).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "app": app_property()
                }
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let (pid, name) = resolve_target(context, &input, self.apps.as_ref())?;
        let shot = self.backend.capture(pid, name.as_deref())?;
        let text = format!(
            "Screenshot of {} ({}×{}). Call ui_click with the target's integer pixel x,y in this exact image: top-left=(0,0), bottom-right=({},{}). Do not normalize each axis to 1000 and do not use macOS screen coordinates.",
            shot.app_name,
            shot.width,
            shot.height,
            shot.width.saturating_sub(1),
            shot.height.saturating_sub(1)
        );
        Ok(ToolResult {
            text,
            created_file: None,
            image: Some(ToolImage {
                media_type: shot.media_type,
                bytes: shot.bytes,
                width: shot.width,
                height: shot.height,
            }),
        })
    }
}

struct UiClick {
    backend: Arc<dyn ScreenshotBackend>,
}

impl Tool for UiClick {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ui_click".into(),
            description: "Click at exact pixel coordinates in the latest take_screenshot image (origin top-left). Use that result's width×height; do not normalize non-square images to 1000×1000. Returns a fresh post-click screenshot for verification. May require user approval. Call take_screenshot first.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "number",
                        "minimum": 0,
                        "description": "Horizontal pixel in the latest screenshot; must be less than its stated width"
                    },
                    "y": {
                        "type": "number",
                        "minimum": 0,
                        "description": "Vertical pixel in the latest screenshot; must be less than its stated height"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["x", "y"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        match (parse_u32(input, "x"), parse_u32(input, "y")) {
            (Ok(x), Ok(y)) => (format!("Click at ({x}, {y})"), app_clause(context)),
            _ => ("Click at (invalid coordinates)".into(), app_clause(context)),
        }
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let x = parse_u32(&input, "x")?;
        let y = parse_u32(&input, "y")?;
        if x == 0 && y == 0 {
            return Err(ToolError::Failed(
                "refusing click (0, 0); that is the top-left image corner, not a typical control. Pick the target's pixel in the latest screenshot."
                    .into(),
            ));
        }
        let click_text = self.backend.click(x, y)?;
        let (text, image) = match self.backend.recapture() {
            Ok(shot) => {
                let text = format!(
                    "{click_text}\n\nPost-click screenshot of {} ({}×{}). Verify the requested control changed before another action.",
                    shot.app_name, shot.width, shot.height
                );
                let image = ToolImage {
                    media_type: shot.media_type,
                    bytes: shot.bytes,
                    width: shot.width,
                    height: shot.height,
                };
                (text, Some(image))
            }
            Err(_) => (
                format!(
                    "{click_text}\n\nPost-click screenshot unavailable; call take_screenshot before another click."
                ),
                None,
            ),
        };
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image,
        })
    }
}

struct ListApps {
    backend: Arc<dyn AppBackend>,
}

impl Tool for ListApps {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_apps".into(),
            description:
                "List running and installed Mac apps with name, bundle id, and running pid.".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn execute(&self, _context: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
        let text = self.backend.list_apps()?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct OpenApp {
    backend: Arc<dyn AppBackend>,
}

impl Tool for OpenApp {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "open_app".into(),
            description: "Open a Mac app by display name or bundle id. May require user approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "App display name"
                    },
                    "bundle_id": {
                        "type": "string",
                        "description": "App bundle identifier"
                    },
                    "ask_user": ask_user_property()
                }
            }),
        }
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let (name, bundle_id) = parse_app_identifiers(input).unwrap_or((None, None));
        let display = app_display_name(name.as_deref(), bundle_id.as_deref());
        (format!("Open {display}"), display)
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let (name, bundle_id) = parse_app_identifiers(&input)?;
        let text = self
            .backend
            .open_app(name.as_deref(), bundle_id.as_deref())?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct FocusApp {
    backend: Arc<dyn AppBackend>,
}

impl Tool for FocusApp {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "focus_app".into(),
            description: "Bring a running Mac app to the front by display name or bundle id. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "App display name"
                    },
                    "bundle_id": {
                        "type": "string",
                        "description": "App bundle identifier"
                    },
                    "ask_user": ask_user_property()
                }
            }),
        }
    }

    fn approval_prompt(&self, _context: &ToolContext, input: &Value) -> (String, String) {
        let (name, bundle_id) = parse_app_identifiers(input).unwrap_or((None, None));
        let display = app_display_name(name.as_deref(), bundle_id.as_deref());
        (format!("Focus {display}"), display)
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let (name, bundle_id) = parse_app_identifiers(&input)?;
        let text = self
            .backend
            .focus_app(name.as_deref(), bundle_id.as_deref())?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct UiType {
    backend: Arc<dyn InputBackend>,
    ax: Arc<dyn AccessibilityBackend>,
}

impl Tool for UiType {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ui_type".into(),
            description: "Type text into the focused field or a node from the latest accessibility snapshot. Do not type passwords; use fill_credential. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to type"
                    },
                    "node_id": {
                        "description": "Optional node id from the latest get_accessibility_snapshot"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["text"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let text = input
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = format!("Type \"{}\"", truncate_ax_text(text));
        (title, app_clause(context))
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let text = input
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("text is required".into()))?;
        let node_id = match input.get("node_id") {
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            Some(Value::Number(value)) => Some(value.to_string()),
            _ => None,
        };
        if node_id
            .as_deref()
            .is_some_and(|id| self.ax.is_secure_node(id))
            || (node_id.is_none() && self.ax.focused_is_secure())
        {
            return Err(ToolError::Failed(
                "won't type into a password field; use fill_credential".into(),
            ));
        }
        let text = self.backend.type_text(text, node_id.as_deref())?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct UiHotkey {
    backend: Arc<dyn InputBackend>,
}

impl Tool for UiHotkey {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ui_hotkey".into(),
            description: "Send a keyboard shortcut such as [\"cmd\", \"c\"]. Prefer ask_user true for send, purchase, or delete shortcuts. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "keys": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 2,
                        "description": "Keys to press together, e.g. [\"cmd\", \"c\"]"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["keys"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let keys = parse_hotkey_keys(input).unwrap_or_default();
        let joined = keys.join("+");
        (format!("Press {joined}"), app_clause(context))
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let keys = parse_hotkey_keys(&input)?;
        let text = self.backend.hotkey(&keys)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct UiScroll {
    backend: Arc<dyn InputBackend>,
}

impl Tool for UiScroll {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ui_scroll".into(),
            description: "Scroll up, down, left, or right in the focused view or at optional coordinates from the latest snapshot. May require user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["up", "down", "left", "right"],
                        "description": "Scroll direction"
                    },
                    "amount": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Scroll amount (default 3)"
                    },
                    "by": {
                        "type": "string",
                        "enum": ["line", "page"],
                        "description": "Scroll unit (default line)"
                    },
                    "node_id": {
                        "description": "Optional node id from the latest get_accessibility_snapshot"
                    },
                    "x": {
                        "type": "number",
                        "minimum": 0,
                        "description": "Optional horizontal pixel for scroll target"
                    },
                    "y": {
                        "type": "number",
                        "minimum": 0,
                        "description": "Optional vertical pixel for scroll target"
                    },
                    "ask_user": ask_user_property()
                },
                "required": ["direction"]
            }),
        }
    }

    fn approval_prompt(&self, context: &ToolContext, input: &Value) -> (String, String) {
        let direction = input
            .get("direction")
            .and_then(Value::as_str)
            .unwrap_or("scroll");
        (format!("Scroll {direction}"), app_clause(context))
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let direction = parse_scroll_direction(&input)?;
        let amount = optional_u32(&input, "amount").unwrap_or(3);
        let by = parse_scroll_by(&input);
        let node_id = match input.get("node_id") {
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            Some(Value::Number(value)) => Some(value.to_string()),
            _ => None,
        };
        let x = optional_u32(&input, "x");
        let y = optional_u32(&input, "y");
        let text = self
            .backend
            .scroll(&direction, amount, &by, node_id.as_deref(), x, y)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::tests::MockBrowser;
    use crate::calendar::MockCalendar;
    use serde_json::json;
    use std::sync::Mutex;

    struct MockApps;

    impl AppBackend for MockApps {
        fn list_apps(&self) -> Result<String, ToolError> {
            Ok("Safari (com.apple.Safari) — running pid 1".into())
        }

        fn open_app(
            &self,
            name: Option<&str>,
            bundle_id: Option<&str>,
        ) -> Result<String, ToolError> {
            Ok(format!("Opened {}", name.or(bundle_id).unwrap_or("app")))
        }

        fn focus_app(
            &self,
            name: Option<&str>,
            bundle_id: Option<&str>,
        ) -> Result<String, ToolError> {
            Ok(format!("Focused {}", name.or(bundle_id).unwrap_or("app")))
        }

        fn resolve_running_app(&self, app: &str) -> Result<(i32, String), ToolError> {
            if app.eq_ignore_ascii_case("Safari") || app == "com.apple.Safari" {
                Ok((42, "Safari".into()))
            } else {
                Err(ToolError::Failed(format!(
                    "no running app matching \"{app}\""
                )))
            }
        }
    }

    struct MockInput {
        typed: Mutex<Vec<(String, Option<String>)>>,
    }

    impl MockInput {
        fn new() -> Self {
            Self {
                typed: Mutex::new(Vec::new()),
            }
        }
    }

    impl InputBackend for MockInput {
        fn type_text(&self, text: &str, node_id: Option<&str>) -> Result<String, ToolError> {
            self.typed
                .lock()
                .expect("lock")
                .push((text.to_string(), node_id.map(str::to_string)));
            Ok(format!("Typed {text}"))
        }

        fn hotkey(&self, keys: &[String]) -> Result<String, ToolError> {
            Ok(format!("Pressed {}", keys.join("+")))
        }

        fn scroll(
            &self,
            direction: &str,
            amount: u32,
            by: &str,
            _node_id: Option<&str>,
            _x: Option<u32>,
            _y: Option<u32>,
        ) -> Result<String, ToolError> {
            Ok(format!("Scrolled {direction} {amount} {by}"))
        }
    }

    struct MockAx {
        snapshot: String,
        pressed: Mutex<Vec<String>>,
        values: Mutex<Vec<(String, String)>>,
        known: Vec<&'static str>,
        secure: Vec<&'static str>,
        focused_secure: bool,
    }

    impl MockAx {
        fn checkout() -> Self {
            Self {
                snapshot: "Application: Safari\n\n[4] AXButton \"Continue\"\n      enabled=true"
                    .into(),
                pressed: Mutex::new(Vec::new()),
                values: Mutex::new(Vec::new()),
                known: vec!["2", "4"],
                secure: vec!["9"],
                focused_secure: false,
            }
        }
    }

    impl AccessibilityBackend for MockAx {
        fn snapshot(
            &self,
            _pid: Option<i32>,
            _app_name: Option<&str>,
        ) -> Result<String, ToolError> {
            Ok(self.snapshot.clone())
        }

        fn press(&self, node_id: &str) -> Result<String, ToolError> {
            if !self.known.contains(&node_id) {
                return Err(ToolError::Failed(
                    "stale or unknown node id. Call get_accessibility_snapshot again.".into(),
                ));
            }
            self.pressed.lock().expect("lock").push(node_id.to_string());
            Ok(format!("Pressed node {node_id}.\n\n{}", self.snapshot))
        }

        fn set_value(&self, node_id: &str, value: &str) -> Result<String, ToolError> {
            if self.is_secure_node(node_id) {
                return Err(ToolError::Failed(
                    "won't set a password field from the snapshot".into(),
                ));
            }
            if !self.known.contains(&node_id) && node_id != "9" {
                return Err(ToolError::Failed(
                    "stale or unknown node id. Call get_accessibility_snapshot again.".into(),
                ));
            }
            self.values
                .lock()
                .expect("lock")
                .push((node_id.to_string(), value.to_string()));
            Ok(format!("Set node {node_id}.\n\n{}", self.snapshot))
        }

        fn set_secure_value(&self, node_id: &str, value: &str) -> Result<String, ToolError> {
            if !self.known.contains(&node_id) && node_id != "9" {
                return Err(ToolError::Failed(
                    "stale or unknown node id. Call get_accessibility_snapshot again.".into(),
                ));
            }
            self.values
                .lock()
                .expect("lock")
                .push((node_id.to_string(), value.to_string()));
            Ok("Filled a password field.".into())
        }

        fn describe_node(&self, node_id: &str) -> Option<String> {
            match node_id {
                "2" => Some("Email".into()),
                "4" => Some("Continue".into()),
                "9" => Some("Password".into()),
                _ => None,
            }
        }

        fn is_secure_node(&self, node_id: &str) -> bool {
            self.secure.contains(&node_id)
        }

        fn focused_is_secure(&self) -> bool {
            self.focused_secure
        }
    }

    struct MockShot {
        captured: Mutex<usize>,
        clicks: Arc<Mutex<Vec<(u32, u32)>>>,
        has_shot: Mutex<bool>,
    }

    impl MockShot {
        fn new() -> Self {
            Self {
                captured: Mutex::new(0),
                clicks: Arc::new(Mutex::new(Vec::new())),
                has_shot: Mutex::new(false),
            }
        }
    }

    impl ScreenshotBackend for MockShot {
        fn capture(
            &self,
            _pid: Option<i32>,
            app_name: Option<&str>,
        ) -> Result<Screenshot, ToolError> {
            *self.captured.lock().expect("lock") += 1;
            *self.has_shot.lock().expect("lock") = true;
            Ok(Screenshot {
                bytes: vec![0x89, b'P', b'N', b'G'],
                media_type: "image/png".into(),
                width: 100,
                height: 50,
                app_name: app_name.unwrap_or("App").to_string(),
            })
        }

        fn click(&self, x: u32, y: u32) -> Result<String, ToolError> {
            if !*self.has_shot.lock().expect("lock") {
                return Err(ToolError::Failed(
                    "no screenshot yet. Call take_screenshot first.".into(),
                ));
            }
            self.clicks.lock().expect("lock").push((x, y));
            Ok(format!("Clicked ({x}, {y}) in App."))
        }

        fn recapture(&self) -> Result<Screenshot, ToolError> {
            if !*self.has_shot.lock().expect("lock") {
                return Err(ToolError::Failed(
                    "no screenshot yet. Call take_screenshot first.".into(),
                ));
            }
            self.capture(None, Some("App"))
        }
    }

    fn mock_apps() -> Arc<dyn AppBackend> {
        Arc::new(MockApps)
    }

    fn ctx() -> ToolContext {
        let mut context = ToolContext::new();
        context.frontmost_name = Some("Safari".into());
        context.frontmost_pid = Some(42);
        context
    }

    fn full_registry(
        ax: Arc<dyn AccessibilityBackend>,
        shot: Arc<dyn ScreenshotBackend>,
    ) -> ToolRegistry {
        computer_and_screenshot_registry(
            ax,
            shot,
            mock_apps(),
            Arc::new(MockInput::new()),
            Arc::new(MockCalendar),
        )
    }

    #[test]
    fn snapshot_and_press_with_numeric_id() {
        let backend = Arc::new(MockAx::checkout());
        let registry = computer_registry(backend, mock_apps());
        let context = ctx();
        let snap = registry
            .execute("get_accessibility_snapshot", &context, json!({}))
            .unwrap();
        assert!(snap.text.contains("Continue"));
        let pressed = registry
            .execute("ui_press", &context, json!({"node_id": 4}))
            .unwrap();
        assert!(pressed.text.contains("Pressed node 4"));
    }

    #[test]
    fn stale_node_id_errors() {
        let err = computer_registry(Arc::new(MockAx::checkout()), mock_apps())
            .execute("ui_press", &ctx(), json!({"node_id": "99"}))
            .unwrap_err();
        assert!(err.to_string().contains("stale"));
    }

    #[test]
    fn press_approval_copy_uses_node_label() {
        let registry = computer_registry(Arc::new(MockAx::checkout()), mock_apps());
        let (title, description) =
            registry.approval_prompt("ui_press", &ctx(), &json!({"node_id": "4"}));
        assert_eq!(title, "Press \"Continue\"");
        assert_eq!(description, "in Safari");
    }

    #[test]
    fn set_value_approval_hides_secure_text() {
        let registry = computer_registry(Arc::new(MockAx::checkout()), mock_apps());
        let (title, _) = registry.approval_prompt(
            "ui_set_value",
            &ctx(),
            &json!({"node_id": "9", "value": "hunter2"}),
        );
        assert_eq!(title, "Fill \"Password\"");
        assert!(!title.contains("hunter2"));
        let (title, _) = registry.approval_prompt(
            "ui_set_value",
            &ctx(),
            &json!({"node_id": "2", "value": "a@b.com"}),
        );
        assert!(title.contains("Email"));
        assert!(title.contains("a@b.com"));
    }

    #[test]
    fn fill_credential_uses_host_login_and_omits_values() {
        let backend = Arc::new(MockAx::checkout());
        let registry = computer_registry(Arc::clone(&backend) as _, mock_apps());
        let mut context = ctx();
        context.fill_username = Some("labuser".into());
        context.fill_password = Some("hunter2".into());
        let result = registry
            .execute(
                "fill_credential",
                &context,
                json!({
                    "credential_ref": "lab.fileserver",
                    "username_node_id": "2",
                    "password_node_id": "9"
                }),
            )
            .unwrap();
        assert!(result.text.contains("lab.fileserver"));
        assert!(!result.text.contains("labuser"));
        assert!(!result.text.contains("hunter2"));
        let filled = backend.values.lock().expect("lock").clone();
        assert_eq!(
            filled,
            vec![
                ("2".into(), "labuser".into()),
                ("9".into(), "hunter2".into())
            ]
        );
    }

    #[test]
    fn fill_credential_without_host_login_errors() {
        let err = computer_registry(Arc::new(MockAx::checkout()), mock_apps())
            .execute(
                "fill_credential",
                &ctx(),
                json!({
                    "credential_ref": "lab.fileserver",
                    "password_node_id": "9"
                }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("collect"));
    }

    #[test]
    fn fill_credential_without_nodes_requires_pending_http_auth() {
        let err = computer_registry(Arc::new(MockAx::checkout()), mock_apps())
            .execute(
                "fill_credential",
                &ctx(),
                json!({ "credential_ref": "lab.fileserver" }),
            )
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("browser_navigate"));
        assert!(message.contains("credential_ref"));
        assert!(!message.contains("hunter2"));
        assert!(!message.contains("labuser"));
    }

    #[test]
    fn fill_credential_http_auth_uses_host_login_and_omits_values() {
        let browser = Arc::new(MockBrowser::with_digest_auth());
        let registry = computer_and_screenshot_registry_with_browser(
            Arc::new(MockAx::checkout()),
            Arc::new(MockShot::new()),
            mock_apps(),
            Arc::new(MockInput::new()),
            Arc::new(MockCalendar),
            Arc::clone(&browser) as _,
        );
        let mut context = ctx();
        context.fill_username = Some("labuser".into());
        context.fill_password = Some("hunter2".into());
        let result = registry
            .execute(
                "fill_credential",
                &context,
                json!({ "credential_ref": "lab.fileserver" }),
            )
            .unwrap();
        assert!(result.text.contains("lab.fileserver"));
        assert!(result.text.contains("files.example.invalid"));
        assert!(!result.text.contains("labuser"));
        assert!(!result.text.contains("hunter2"));
        assert_eq!(
            browser.http_auth_fills.lock().expect("lock").clone(),
            vec![("labuser".into(), "hunter2".into())]
        );
        assert!(browser.pending_auth.lock().expect("lock").is_none());
    }

    #[test]
    fn fill_credential_http_auth_without_host_login_errors() {
        let browser = Arc::new(MockBrowser::with_digest_auth());
        let registry = computer_and_screenshot_registry_with_browser(
            Arc::new(MockAx::checkout()),
            Arc::new(MockShot::new()),
            mock_apps(),
            Arc::new(MockInput::new()),
            Arc::new(MockCalendar),
            browser,
        );
        let err = registry
            .execute(
                "fill_credential",
                &ctx(),
                json!({ "credential_ref": "lab.fileserver" }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("collect"));
    }

    #[test]
    fn ui_set_value_refuses_secure_fields() {
        let err = computer_registry(Arc::new(MockAx::checkout()), mock_apps())
            .execute(
                "ui_set_value",
                &ctx(),
                json!({"node_id": "9", "value": "hunter2"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("password"));
        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn ui_type_refuses_secure_fields() {
        let err = full_registry(Arc::new(MockAx::checkout()), Arc::new(MockShot::new()))
            .execute(
                "ui_type",
                &ctx(),
                json!({"text": "hunter2", "node_id": "9"}),
            )
            .unwrap_err();
        assert!(err.to_string().contains("fill_credential"));
        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn ui_type_refuses_focused_secure_field() {
        let err = full_registry(
            Arc::new(MockAx {
                focused_secure: true,
                ..MockAx::checkout()
            }),
            Arc::new(MockShot::new()),
        )
        .execute("ui_type", &ctx(), json!({"text": "hunter2"}))
        .unwrap_err();
        assert!(err.to_string().contains("fill_credential"));
        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn screenshot_then_click() {
        let clicks = Arc::new(Mutex::new(Vec::new()));
        let shot = Arc::new(MockShot {
            captured: Mutex::new(0),
            clicks: Arc::clone(&clicks),
            has_shot: Mutex::new(false),
        });
        let registry = full_registry(
            Arc::new(MockAx::checkout()),
            Arc::clone(&shot) as Arc<dyn ScreenshotBackend>,
        );
        let context = ctx();
        let result = registry
            .execute("take_screenshot", &context, json!({}))
            .unwrap();
        assert!(result.text.contains("100×50"));
        let image = result.image.expect("image");
        assert_eq!(image.width, 100);
        assert_eq!(image.height, 50);
        assert_eq!(image.media_type, "image/png");
        let clicked = registry
            .execute("ui_click", &context, json!({"x": 10, "y": 20}))
            .unwrap();
        assert!(clicked.text.contains("Clicked (10, 20)"));
        assert!(clicked.text.contains("Post-click screenshot"));
        assert!(clicked.image.is_some());
        assert_eq!(*clicks.lock().unwrap(), vec![(10, 20)]);
        assert_eq!(*shot.captured.lock().unwrap(), 2);
    }

    #[test]
    fn click_without_screenshot_errors() {
        let err = full_registry(Arc::new(MockAx::checkout()), Arc::new(MockShot::new()))
            .execute("ui_click", &ctx(), json!({"x": 1, "y": 2}))
            .unwrap_err();
        assert!(err.to_string().contains("take_screenshot"));
    }

    #[test]
    fn click_approval_copy_includes_coordinates() {
        let registry = full_registry(Arc::new(MockAx::checkout()), Arc::new(MockShot::new()));
        let (title, description) =
            registry.approval_prompt("ui_click", &ctx(), &json!({"x": 40, "y": 80}));
        assert_eq!(title, "Click at (40, 80)");
        assert_eq!(description, "in Safari");
        let (title, _) =
            registry.approval_prompt("ui_click", &ctx(), &json!({"x": 40.2, "y": 80.7}));
        assert_eq!(title, "Click at (40, 81)");
    }

    #[test]
    fn click_rejects_origin_corner() {
        let shot = Arc::new(MockShot {
            captured: Mutex::new(0),
            clicks: Arc::new(Mutex::new(Vec::new())),
            has_shot: Mutex::new(true),
        });
        let err = full_registry(
            Arc::new(MockAx::checkout()),
            shot as Arc<dyn ScreenshotBackend>,
        )
        .execute("ui_click", &ctx(), json!({"x": 0, "y": 0}))
        .unwrap_err();
        assert!(err.to_string().contains("(0, 0)"));
    }

    #[test]
    fn click_accepts_float_coordinates() {
        let clicks = Arc::new(Mutex::new(Vec::new()));
        let shot = Arc::new(MockShot {
            captured: Mutex::new(0),
            clicks: Arc::clone(&clicks),
            has_shot: Mutex::new(true),
        });
        full_registry(
            Arc::new(MockAx::checkout()),
            shot as Arc<dyn ScreenshotBackend>,
        )
        .execute("ui_click", &ctx(), json!({"x": 12.4, "y": 18.6}))
        .unwrap();
        assert_eq!(*clicks.lock().unwrap(), vec![(12, 19)]);
    }

    #[test]
    fn list_apps_and_open_app_with_mocks() {
        let registry = full_registry(Arc::new(MockAx::checkout()), Arc::new(MockShot::new()));
        let listed = registry.execute("list_apps", &ctx(), json!({})).unwrap();
        assert!(listed.text.contains("Safari"));
        let opened = registry
            .execute("open_app", &ctx(), json!({"name": "Safari"}))
            .unwrap();
        assert!(opened.text.contains("Opened Safari"));
    }

    #[test]
    fn ui_type_with_mock() {
        let input = Arc::new(MockInput::new());
        let registry = computer_and_screenshot_registry(
            Arc::new(MockAx::checkout()),
            Arc::new(MockShot::new()),
            mock_apps(),
            Arc::clone(&input) as Arc<dyn InputBackend>,
            Arc::new(MockCalendar),
        );
        let result = registry
            .execute("ui_type", &ctx(), json!({"text": "hello"}))
            .unwrap();
        assert!(result.text.contains("Typed hello"));
        assert_eq!(input.typed.lock().unwrap()[0], ("hello".into(), None));
    }
}
