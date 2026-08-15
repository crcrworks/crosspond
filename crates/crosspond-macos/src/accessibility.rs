use std::collections::HashMap;
use std::sync::Mutex;

use crosspond_tools::{AccessibilityBackend, ToolError};

use crate::context::CROSSPOND_BUNDLE_ID;

pub struct MacOsAccessibility {
    live: Mutex<Option<LiveSnapshot>>,
}

struct LiveSnapshot {
    pid: i32,
    app_name: String,
    nodes: HashMap<u32, LiveNode>,
}

struct LiveNode {
    #[cfg(target_os = "macos")]
    element: AxElement,
    label: String,
    secure: bool,
}

#[cfg(target_os = "macos")]
struct AxElement(core_foundation::base::CFType);

// SAFETY: AXUIElement is a CFType. Tool calls are serialized through `live`.
#[cfg(target_os = "macos")]
unsafe impl Send for AxElement {}

impl MacOsAccessibility {
    pub fn new() -> Self {
        Self {
            live: Mutex::new(None),
        }
    }
}

impl Default for MacOsAccessibility {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessibilityBackend for MacOsAccessibility {
    fn snapshot(&self, pid: Option<i32>, app_name: Option<&str>) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (pid, app_name);
            return Err(ToolError::Failed(
                "Accessibility is only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            snapshot_macos(self, pid, app_name)
        }
    }

    fn press(&self, node_id: &str) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = node_id;
            return Err(ToolError::Failed(
                "Accessibility is only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            action_macos(self, node_id, AxAction::Press)
        }
    }

    fn set_value(&self, node_id: &str, value: &str) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (node_id, value);
            return Err(ToolError::Failed(
                "Accessibility is only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            action_macos(self, node_id, AxAction::SetValue(value))
        }
    }

    fn describe_node(&self, node_id: &str) -> Option<String> {
        let id = node_id.parse().ok()?;
        let live = self.live.lock().ok()?;
        live.as_ref()
            .and_then(|snapshot| snapshot.nodes.get(&id))
            .map(|node| node.label.clone())
    }

    fn is_secure_node(&self, node_id: &str) -> bool {
        let Ok(id) = node_id.parse() else {
            return false;
        };
        let Ok(live) = self.live.lock() else {
            return false;
        };
        live.as_ref()
            .and_then(|snapshot| snapshot.nodes.get(&id))
            .is_some_and(|node| node.secure)
    }
}

#[cfg(target_os = "macos")]
enum AxAction<'a> {
    Press,
    SetValue(&'a str),
}

#[cfg(target_os = "macos")]
fn snapshot_macos(
    backend: &MacOsAccessibility,
    pid: Option<i32>,
    app_name: Option<&str>,
) -> Result<String, ToolError> {
    if !crate::ax::is_trusted() {
        return Err(not_trusted());
    }
    let (pid, name) = resolve_target(pid, app_name)?;
    take_snapshot(backend, pid, &name)
}

#[cfg(target_os = "macos")]
fn action_macos(
    backend: &MacOsAccessibility,
    node_id: &str,
    action: AxAction<'_>,
) -> Result<String, ToolError> {
    if !crate::ax::is_trusted() {
        return Err(not_trusted());
    }
    let id: u32 = node_id
        .parse()
        .map_err(|_| ToolError::Failed("node_id must be a number".into()))?;
    let pressed = matches!(action, AxAction::Press);
    let (pid, app_name, label) = {
        let live = backend
            .live
            .lock()
            .map_err(|_| ToolError::Failed("accessibility state is unavailable".into()))?;
        let snapshot = live.as_ref().ok_or_else(stale_node)?;
        let node = snapshot.nodes.get(&id).ok_or_else(stale_node)?;
        match action {
            AxAction::Press => crate::ax::ax_press(&node.element.0).map_err(ToolError::Failed)?,
            AxAction::SetValue(value) => {
                crate::ax::ax_set_value(&node.element.0, value).map_err(ToolError::Failed)?
            }
        }
        (snapshot.pid, snapshot.app_name.clone(), node.label.clone())
    };
    std::thread::sleep(std::time::Duration::from_millis(50));
    let tree = take_snapshot(backend, pid, &app_name)?;
    let verb = if pressed {
        format!("Pressed {label}.")
    } else {
        format!("Set {label}.")
    };
    Ok(format!("{verb}\n\n{tree}"))
}

#[cfg(target_os = "macos")]
fn take_snapshot(
    backend: &MacOsAccessibility,
    pid: i32,
    app_name: &str,
) -> Result<String, ToolError> {
    crate::ax::enable_background_ax(pid);
    let root = crate::ax::snapshot_root(pid).ok_or_else(|| {
        ToolError::Failed("could not read the Accessibility tree of the frontmost app".into())
    })?;
    let mut walker = Walker {
        next_id: 1,
        count: 0,
        truncated: false,
        nodes: HashMap::new(),
    };
    let outline = walker.walk(root, 0);
    let mut roots = Vec::new();
    if let Some(node) = outline {
        roots.push(node);
    }
    let text = crosspond_tools::render_ax_outline(app_name, &roots, walker.truncated);
    let mut live = backend
        .live
        .lock()
        .map_err(|_| ToolError::Failed("accessibility state is unavailable".into()))?;
    *live = Some(LiveSnapshot {
        pid,
        app_name: app_name.to_string(),
        nodes: walker.nodes,
    });
    Ok(text)
}

#[cfg(target_os = "macos")]
struct Walker {
    next_id: u32,
    count: usize,
    truncated: bool,
    nodes: HashMap<u32, LiveNode>,
}

#[cfg(target_os = "macos")]
impl Walker {
    fn walk(
        &mut self,
        element: core_foundation::base::CFType,
        depth: usize,
    ) -> Option<crosspond_tools::AxOutlineNode> {
        if self.count >= crosspond_tools::MAX_AX_NODES {
            self.truncated = true;
            return None;
        }
        let role = crate::ax::ax_string(&element, "AXRole").unwrap_or_else(|| "AXUnknown".into());
        let subrole = crate::ax::ax_string(&element, "AXSubrole").unwrap_or_default();
        if skip_ax_role(&role) || crate::ax::is_chrome_subrole(&subrole) {
            return None;
        }
        self.count += 1;
        let id = self.next_id;
        self.next_id += 1;

        let title = crate::ax::ax_string(&element, "AXTitle")
            .or_else(|| crate::ax::ax_string(&element, "AXDescription"))
            .map(|title| crosspond_tools::truncate_ax_text(&title));
        let secure = role == "AXSecureTextField";
        let value = if secure {
            Some("••••".into())
        } else {
            crate::ax::ax_string_raw(&element, "AXValue")
                .map(|text| crosspond_tools::truncate_ax_text(&text))
        };
        let enabled = crate::ax::ax_bool(&element, "AXEnabled");
        let focused = crate::ax::ax_bool(&element, "AXFocused").unwrap_or(false);
        let label = title
            .clone()
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| role.clone());

        let children_raw = crate::ax::ax_children(&element);
        self.nodes.insert(
            id,
            LiveNode {
                element: AxElement(element),
                label,
                secure,
            },
        );

        let mut children = Vec::new();
        let mut truncated_children = false;
        let deep_web = role == "AXWebArea" && children_raw.len() > 12;
        if depth >= crosspond_tools::MAX_AX_DEPTH || deep_web {
            truncated_children = !children_raw.is_empty();
            if truncated_children {
                self.truncated = true;
            }
        } else {
            for child in children_raw {
                if self.count >= crosspond_tools::MAX_AX_NODES {
                    self.truncated = true;
                    truncated_children = true;
                    break;
                }
                if let Some(node) = self.walk(child, depth + 1) {
                    children.push(node);
                }
            }
        }

        Some(crosspond_tools::AxOutlineNode {
            id,
            role,
            title,
            value,
            enabled,
            focused,
            truncated_children,
            children,
        })
    }
}

#[cfg(target_os = "macos")]
fn resolve_target(pid: Option<i32>, app_name: Option<&str>) -> Result<(i32, String), ToolError> {
    if let Some(pid) = pid
        && pid > 0
        && pid != std::process::id() as i32
    {
        let name = app_name
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("pid {pid}"));
        return Ok((pid, name));
    }
    let Some(app) = crate::context::frontmost_app() else {
        return Err(ToolError::Failed(
            "no target app. Open another app, then press Option+Space.".into(),
        ));
    };
    if app.bundle_id == CROSSPOND_BUNDLE_ID || app.pid == std::process::id() as i32 {
        return Err(ToolError::Failed(
            "no target app. Open another app, then press Option+Space.".into(),
        ));
    }
    Ok((app.pid, app.name))
}

#[cfg(target_os = "macos")]
fn not_trusted() -> ToolError {
    ToolError::Failed(
        "Accessibility is off. Enable Crosspond in System Settings → Privacy & Security → Accessibility, then try again.".into(),
    )
}

fn stale_node() -> ToolError {
    ToolError::Failed("stale or unknown node id. Call get_accessibility_snapshot again.".into())
}

pub(crate) fn skip_ax_role(role: &str) -> bool {
    matches!(
        role,
        "AXMenuBar"
            | "AXMenu"
            | "AXMenuBarItem"
            | "AXMenuItem"
            | "AXScrollBar"
            | "AXDockItem"
            | "AXCloseButton"
            | "AXMinimizeButton"
            | "AXZoomButton"
            | "AXFullScreenButton"
            | "AXGrowArea"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_chrome_roles() {
        assert!(skip_ax_role("AXMenuBar"));
        assert!(skip_ax_role("AXMenuItem"));
        assert!(skip_ax_role("AXScrollBar"));
        assert!(skip_ax_role("AXCloseButton"));
        assert!(skip_ax_role("AXMinimizeButton"));
        assert!(!skip_ax_role("AXButton"));
        assert!(!skip_ax_role("AXTextField"));
        assert!(!skip_ax_role("AXWindow"));
    }
}
