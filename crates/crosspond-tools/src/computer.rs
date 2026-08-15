use std::sync::Arc;

use serde_json::{Value, json};

use crate::ax_outline::truncate_ax_text;
use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult, truncate_output};

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

pub fn computer_registry(backend: Arc<dyn AccessibilityBackend>) -> ToolRegistry {
    let mut registry = crate::filesystem_registry();
    register_computer_tools(&mut registry, backend);
    registry
}

fn parse_node_id(input: &Value) -> Result<String, ToolError> {
    match input.get("node_id") {
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(Value::Number(value)) => Ok(value.to_string()),
        _ => Err(ToolError::Failed("node_id is required".into())),
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
            description: "Read a compact Accessibility tree of the app the user was in when they opened Crosspond. Node ids are only valid until the next snapshot or UI action.".into(),
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
            description: "Press a button or other pressable control from the latest accessibility snapshot. Requires user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "description": "Node id from the latest get_accessibility_snapshot (string or number)"
                    }
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
            description: "Set the value of a text field from the latest accessibility snapshot. Requires user approval.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "description": "Node id from the latest get_accessibility_snapshot (string or number)"
                    },
                    "value": {
                        "type": "string",
                        "description": "Text to put in the field"
                    }
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
}
