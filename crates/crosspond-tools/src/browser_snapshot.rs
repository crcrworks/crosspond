//! Compact Chromium accessibility outline for `browser_snapshot`.

use serde::Deserialize;
use serde_json::Value;

use crate::ax_outline::{MAX_AX_NODES, truncate_ax_text};

const INTERACTIVE: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "combobox",
    "checkbox",
    "radio",
    "slider",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "switch",
    "option",
    "listbox",
    "spinbutton",
    "treeitem",
];

const CONTEXT: &[&str] = &[
    "heading",
    "webarea",
    "document",
    "dialog",
    "alert",
    "status",
    "navigation",
    "main",
];

#[derive(Clone, Debug)]
pub struct BoundRef {
    pub id: String,
    pub role: String,
    pub name: String,
    pub backend_dom_node_id: i64,
    pub secret: bool,
}

#[derive(Clone, Debug)]
pub struct SnapshotRender {
    pub text: String,
    pub refs: Vec<BoundRef>,
    pub epoch: String,
}

#[derive(Debug, Deserialize)]
struct AxTree {
    #[serde(default)]
    nodes: Vec<AxNode>,
}

#[derive(Debug, Deserialize)]
struct AxNode {
    #[serde(rename = "nodeId", default)]
    node_id: String,
    #[serde(default)]
    ignored: bool,
    role: Option<AxValue>,
    name: Option<AxValue>,
    value: Option<AxValue>,
    #[serde(rename = "backendDOMNodeId")]
    backend_dom_node_id: Option<i64>,
    #[serde(default, rename = "childIds")]
    child_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AxValue {
    value: Option<Value>,
}

impl AxValue {
    fn as_str(&self) -> String {
        match &self.value {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Bool(flag)) => flag.to_string(),
            Some(Value::Number(num)) => num.to_string(),
            _ => String::new(),
        }
    }
}

pub fn render_cdp_ax_tree(
    title: &str,
    url: &str,
    epoch: &str,
    tree: &Value,
) -> Result<SnapshotRender, String> {
    let parsed: AxTree =
        serde_json::from_value(tree.clone()).map_err(|err| format!("accessibility tree: {err}"))?;
    let by_id: std::collections::HashMap<&str, &AxNode> = parsed
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect();
    let roots: Vec<&AxNode> = parsed
        .nodes
        .iter()
        .filter(|node| {
            !parsed
                .nodes
                .iter()
                .any(|other| other.child_ids.iter().any(|id| id == &node.node_id))
        })
        .collect();

    let mut lines = vec![
        format!("Page: {title}"),
        format!("URL: {url}"),
        "Refs are valid until the next browser_snapshot or navigation.".into(),
        String::new(),
    ];
    let mut refs = Vec::new();
    let mut visited = 0usize;
    let mut truncated = false;
    {
        let mut out = RenderOut {
            epoch,
            lines: &mut lines,
            refs: &mut refs,
            visited: &mut visited,
            truncated: &mut truncated,
        };
        for root in roots {
            render_node(root, &by_id, 0, &mut out);
        }
    }
    if truncated {
        lines.push("… truncated".into());
    }
    Ok(SnapshotRender {
        text: lines.join("\n"),
        refs,
        epoch: epoch.to_string(),
    })
}

struct RenderOut<'a> {
    epoch: &'a str,
    lines: &'a mut Vec<String>,
    refs: &'a mut Vec<BoundRef>,
    visited: &'a mut usize,
    truncated: &'a mut bool,
}

fn render_node(
    node: &AxNode,
    by_id: &std::collections::HashMap<&str, &AxNode>,
    depth: usize,
    out: &mut RenderOut<'_>,
) {
    if *out.visited >= MAX_AX_NODES {
        *out.truncated = true;
        return;
    }
    *out.visited += 1;
    let role = node
        .role
        .as_ref()
        .map(AxValue::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = node.name.as_ref().map(AxValue::as_str).unwrap_or_default();
    let value = node.value.as_ref().map(AxValue::as_str).unwrap_or_default();
    let show = !node.ignored
        && (is_interactive(&role) || is_context(&role) && !name.is_empty() || depth == 0);
    if show {
        let indent = "  ".repeat(depth.min(8));
        let mut line = format!("{indent}{role}");
        if !name.is_empty() {
            line.push_str(&format!(" \"{}\"", truncate_ax_text(&name)));
        }
        if is_interactive(&role)
            && let Some(backend) = node.backend_dom_node_id
        {
            let index = out.refs.len() + 1;
            let id = format!("{}-e{index}", out.epoch);
            let secret = is_secret_field(&role, &name);
            line.push_str(&format!(" [{id}]"));
            if !value.is_empty() {
                let shown = if secret {
                    "••••".into()
                } else {
                    truncate_ax_text(&value)
                };
                line.push_str(&format!(" value={shown}"));
            } else if secret {
                line.push_str(" value=••••");
            }
            out.refs.push(BoundRef {
                id: id.clone(),
                role: role.clone(),
                name,
                backend_dom_node_id: backend,
                secret,
            });
        }
        out.lines.push(line);
    }
    let next_depth = if show { depth + 1 } else { depth };
    for child_id in &node.child_ids {
        if let Some(child) = by_id.get(child_id.as_str()) {
            render_node(child, by_id, next_depth, out);
        }
    }
}

fn is_interactive(role: &str) -> bool {
    INTERACTIVE.contains(&role)
}

fn is_context(role: &str) -> bool {
    CONTEXT.contains(&role)
}

pub fn is_secret_field(role: &str, name: &str) -> bool {
    let blob = format!("{role} {name}").to_ascii_lowercase();
    blob.contains("password")
        || blob.contains("otp")
        || blob.contains("one-time")
        || blob.contains("email")
        || blob.contains("e-mail")
        || blob.contains("phone")
        || blob.contains("tel")
        || blob.contains("credit")
        || blob.contains("card number")
        || blob.contains("cvv")
}

pub fn parse_ref(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("ref is required".into());
    }
    let value = trimmed.strip_prefix('@').unwrap_or(trimmed);
    if !value.contains("-e") {
        return Err("stale or unknown ref. Call browser_snapshot again.".into());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_interactive_refs_and_redacts_secrets() {
        let tree = json!({
            "nodes": [
                {
                    "nodeId": "1",
                    "role": { "value": "WebArea" },
                    "name": { "value": "Login" },
                    "childIds": ["2", "3", "4"]
                },
                {
                    "nodeId": "2",
                    "role": { "value": "heading" },
                    "name": { "value": "Sign in" },
                    "childIds": []
                },
                {
                    "nodeId": "3",
                    "role": { "value": "textbox" },
                    "name": { "value": "Password" },
                    "value": { "value": "hunter2" },
                    "backendDOMNodeId": 11,
                    "childIds": []
                },
                {
                    "nodeId": "4",
                    "role": { "value": "button" },
                    "name": { "value": "Continue" },
                    "backendDOMNodeId": 12,
                    "childIds": []
                }
            ]
        });
        let rendered =
            render_cdp_ax_tree("Login", "https://example.com/login", "a1f3", &tree).unwrap();
        assert!(rendered.text.contains("heading \"Sign in\""));
        assert!(rendered.text.contains("button \"Continue\" [a1f3-e2]"));
        assert!(rendered.text.contains("textbox \"Password\" [a1f3-e1]"));
        assert!(rendered.text.contains("value=••••"));
        assert!(!rendered.text.contains("hunter2"));
        assert_eq!(rendered.refs.len(), 2);
        assert!(rendered.refs[0].secret);
        assert!(!rendered.refs[1].secret);
        assert_eq!(rendered.refs[1].backend_dom_node_id, 12);
    }

    #[test]
    fn parse_ref_accepts_at_prefix() {
        assert_eq!(parse_ref(" @a1f3-e2 ").unwrap(), "a1f3-e2");
        assert!(parse_ref("e2").unwrap_err().contains("stale"));
    }
}
