use std::sync::Arc;

use serde_json::{Value, json};

use crate::ax_outline::truncate_ax_text;
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
    fn describe_node(&self, node_id: &str) -> Option<String>;
    fn is_secure_node(&self, _node_id: &str) -> bool {
        false
    }
}

/// Window screenshot and image-coordinate click. Implemented in `crosspond-macos`.
pub trait ScreenshotBackend: Send + Sync {
    fn capture(&self, pid: Option<i32>, app_name: Option<&str>) -> Result<Screenshot, ToolError>;
    fn click(&self, x: u32, y: u32) -> Result<String, ToolError>;
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
) {
    registry.register(Arc::new(GetAccessibilitySnapshot {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(UiPress {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(UiSetValue { backend }));
}

pub fn register_screenshot_tools(registry: &mut ToolRegistry, backend: Arc<dyn ScreenshotBackend>) {
    registry.register(Arc::new(TakeScreenshot {
        backend: Arc::clone(&backend),
    }));
    registry.register(Arc::new(UiClick { backend }));
}

pub fn computer_registry(backend: Arc<dyn AccessibilityBackend>) -> ToolRegistry {
    let mut registry = crate::filesystem_registry();
    register_computer_tools(&mut registry, backend);
    registry
}

pub fn computer_and_screenshot_registry(
    ax: Arc<dyn AccessibilityBackend>,
    screenshot: Arc<dyn ScreenshotBackend>,
) -> ToolRegistry {
    let mut registry = computer_registry(ax);
    register_screenshot_tools(&mut registry, screenshot);
    registry
}

fn ask_user_property() -> Value {
    json!({
        "type": "boolean",
        "description": "When UI approval is AI, true asks the user first and false runs immediately. Ignored in Auto and Manual modes. Prefer true when unsure."
    })
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
            // Models often emit `70.0`; serde stores that as f64 and as_u64() fails.
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

fn app_clause(context: &ToolContext) -> String {
    match context.frontmost_name.as_deref() {
        Some(name) if !name.is_empty() => format!("in {name}"),
        _ => "in the frontmost app".into(),
    }
}

struct GetAccessibilitySnapshot {
    backend: Arc<dyn AccessibilityBackend>,
}

impl Tool for GetAccessibilitySnapshot {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_accessibility_snapshot".into(),
            description: "Read a compact Accessibility tree of the app the user was in when they opened Crosspond. Prefer this before screenshots when the target has a visible name (buttons, channels, tabs). Node ids are only valid until the next snapshot or UI action.".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn execute(&self, context: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
        let text = self
            .backend
            .snapshot(context.frontmost_pid, context.frontmost_name.as_deref())?;
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
            description: "Set the value of a text field from the latest accessibility snapshot. May require user approval.".into(),
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

struct TakeScreenshot {
    backend: Arc<dyn ScreenshotBackend>,
}

impl Tool for TakeScreenshot {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "take_screenshot".into(),
            description: "Capture a screenshot of the window of the app the user was in when they opened Crosspond. Use only when Accessibility has no useful label for the target (canvas, unlabeled icons, some web pages). ui_click uses exact pixel coordinates in this image (origin top-left).".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn execute(&self, context: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
        let shot = self
            .backend
            .capture(context.frontmost_pid, context.frontmost_name.as_deref())?;
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

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let x = parse_u32(&input, "x")?;
        let y = parse_u32(&input, "y")?;
        if x == 0 && y == 0 {
            return Err(ToolError::Failed(
                "refusing click (0, 0); that is the top-left image corner, not a typical control. Pick the target's pixel in the latest screenshot."
                    .into(),
            ));
        }
        let click_text = self.backend.click(x, y)?;
        let (text, image) = match self
            .backend
            .capture(context.frontmost_pid, context.frontmost_name.as_deref())
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use serde_json::json;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockAx {
        snapshot: String,
        pressed: Mutex<Vec<String>>,
        values: Mutex<Vec<(String, String)>>,
        known: Vec<&'static str>,
        secure: Vec<&'static str>,
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
    }

    fn temp_workspace() -> Workspace {
        let root = std::env::temp_dir().join(format!("crosspond-ax-{}", Uuid::new_v4()));
        Workspace::create(root).unwrap()
    }

    fn ctx(workspace: &Workspace) -> ToolContext {
        let mut context = ToolContext::new(workspace.clone());
        context.frontmost_name = Some("Safari".into());
        context.frontmost_pid = Some(42);
        context
    }

    #[test]
    fn snapshot_and_press_with_numeric_id() {
        let workspace = temp_workspace();
        let backend = Arc::new(MockAx::checkout());
        let registry = computer_registry(backend);
        let context = ctx(&workspace);
        let snap = registry
            .execute("get_accessibility_snapshot", &context, json!({}))
            .unwrap();
        assert!(snap.text.contains("Continue"));
        let pressed = registry
            .execute("ui_press", &context, json!({"node_id": 4}))
            .unwrap();
        assert!(pressed.text.contains("Pressed node 4"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn stale_node_id_errors() {
        let workspace = temp_workspace();
        let err = computer_registry(Arc::new(MockAx::checkout()))
            .execute("ui_press", &ctx(&workspace), json!({"node_id": "99"}))
            .unwrap_err();
        assert!(err.to_string().contains("stale"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn press_approval_copy_uses_node_label() {
        let workspace = temp_workspace();
        let registry = computer_registry(Arc::new(MockAx::checkout()));
        let (title, description) =
            registry.approval_prompt("ui_press", &ctx(&workspace), &json!({"node_id": "4"}));
        assert_eq!(title, "Press \"Continue\"");
        assert_eq!(description, "in Safari");
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn set_value_approval_hides_secure_text() {
        let workspace = temp_workspace();
        let registry = computer_registry(Arc::new(MockAx::checkout()));
        let (title, _) = registry.approval_prompt(
            "ui_set_value",
            &ctx(&workspace),
            &json!({"node_id": "9", "value": "hunter2"}),
        );
        assert_eq!(title, "Fill \"Password\"");
        assert!(!title.contains("hunter2"));
        let (title, _) = registry.approval_prompt(
            "ui_set_value",
            &ctx(&workspace),
            &json!({"node_id": "2", "value": "a@b.com"}),
        );
        assert!(title.contains("Email"));
        assert!(title.contains("a@b.com"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn screenshot_then_click() {
        let workspace = temp_workspace();
        let clicks = Arc::new(Mutex::new(Vec::new()));
        let shot = Arc::new(MockShot {
            captured: Mutex::new(0),
            clicks: Arc::clone(&clicks),
            has_shot: Mutex::new(false),
        });
        let registry = computer_and_screenshot_registry(
            Arc::new(MockAx::checkout()),
            Arc::clone(&shot) as Arc<dyn ScreenshotBackend>,
        );
        let context = ctx(&workspace);
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
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn click_without_screenshot_errors() {
        let workspace = temp_workspace();
        let err = computer_and_screenshot_registry(
            Arc::new(MockAx::checkout()),
            Arc::new(MockShot::new()),
        )
        .execute("ui_click", &ctx(&workspace), json!({"x": 1, "y": 2}))
        .unwrap_err();
        assert!(err.to_string().contains("take_screenshot"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn click_approval_copy_includes_coordinates() {
        let workspace = temp_workspace();
        let registry = computer_and_screenshot_registry(
            Arc::new(MockAx::checkout()),
            Arc::new(MockShot::new()),
        );
        let (title, description) =
            registry.approval_prompt("ui_click", &ctx(&workspace), &json!({"x": 40, "y": 80}));
        assert_eq!(title, "Click at (40, 80)");
        assert_eq!(description, "in Safari");
        let (title, _) =
            registry.approval_prompt("ui_click", &ctx(&workspace), &json!({"x": 40.2, "y": 80.7}));
        assert_eq!(title, "Click at (40, 81)");
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn click_rejects_origin_corner() {
        let workspace = temp_workspace();
        let shot = Arc::new(MockShot {
            captured: Mutex::new(0),
            clicks: Arc::new(Mutex::new(Vec::new())),
            has_shot: Mutex::new(true),
        });
        let err = computer_and_screenshot_registry(
            Arc::new(MockAx::checkout()),
            shot as Arc<dyn ScreenshotBackend>,
        )
        .execute("ui_click", &ctx(&workspace), json!({"x": 0, "y": 0}))
        .unwrap_err();
        assert!(err.to_string().contains("(0, 0)"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn click_accepts_float_coordinates() {
        let workspace = temp_workspace();
        let clicks = Arc::new(Mutex::new(Vec::new()));
        let shot = Arc::new(MockShot {
            captured: Mutex::new(0),
            clicks: Arc::clone(&clicks),
            has_shot: Mutex::new(true),
        });
        computer_and_screenshot_registry(
            Arc::new(MockAx::checkout()),
            shot as Arc<dyn ScreenshotBackend>,
        )
        .execute("ui_click", &ctx(&workspace), json!({"x": 12.4, "y": 18.6}))
        .unwrap();
        assert_eq!(*clicks.lock().unwrap(), vec![(12, 19)]);
        let _ = std::fs::remove_dir_all(&workspace.root);
    }
}
