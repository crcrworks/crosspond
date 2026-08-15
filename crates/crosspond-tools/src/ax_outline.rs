/// Caps so an Accessibility dump cannot blow the model context window.
pub const MAX_AX_DEPTH: usize = 8;
pub const MAX_AX_NODES: usize = 80;
pub const MAX_AX_TEXT_CHARS: usize = 120;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AxOutlineNode {
    pub id: u32,
    pub role: String,
    pub title: Option<String>,
    pub value: Option<String>,
    pub enabled: Option<bool>,
    pub focused: bool,
    pub truncated_children: bool,
    pub children: Vec<AxOutlineNode>,
}

pub fn truncate_ax_text(text: &str) -> String {
    let total = text.chars().count();
    if total <= MAX_AX_TEXT_CHARS {
        return text.to_string();
    }
    let mut body: String = text.chars().take(MAX_AX_TEXT_CHARS).collect();
    body.push('…');
    body
}

/// Compact tree for the model. Node ids are temporary and only valid
/// for the snapshot that produced this outline.
pub fn render_ax_outline(app_name: &str, roots: &[AxOutlineNode], truncated: bool) -> String {
    let mut lines = vec![format!("Application: {app_name}"), String::new()];
    for root in roots {
        render_node(root, 0, &mut lines);
    }
    if truncated {
        lines.push("… truncated".into());
    }
    lines.join("\n")
}

fn render_node(node: &AxOutlineNode, depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let mut head = format!("{indent}[{}] {}", node.id, node.role);
    if let Some(title) = &node.title
        && !title.is_empty()
    {
        head.push_str(&format!(" \"{title}\""));
    }
    if node.truncated_children {
        head.push_str(" (truncated)");
    }
    if node.focused {
        head.push_str(" focused=true");
    }
    lines.push(head);

    let extra = format!("{}    ", "  ".repeat(depth));
    if let Some(value) = &node.value {
        lines.push(format!("{extra}value=\"{value}\""));
    }
    if let Some(enabled) = node.enabled
        && (!enabled || is_button_like(&node.role))
    {
        lines.push(format!("{extra}enabled={enabled}"));
    }
    for child in &node.children {
        render_node(child, depth + 1, lines);
    }
}

fn is_button_like(role: &str) -> bool {
    matches!(
        role,
        "AXButton" | "AXCheckBox" | "AXRadioButton" | "AXPopUpButton" | "AXMenuButton"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_checkout_style_tree() {
        let tree = vec![AxOutlineNode {
            id: 1,
            role: "AXWindow".into(),
            title: Some("Checkout".into()),
            children: vec![
                AxOutlineNode {
                    id: 2,
                    role: "AXTextField".into(),
                    title: Some("Email".into()),
                    value: Some(String::new()),
                    ..AxOutlineNode::default()
                },
                AxOutlineNode {
                    id: 3,
                    role: "AXTextField".into(),
                    title: Some("Company".into()),
                    value: Some(String::new()),
                    ..AxOutlineNode::default()
                },
                AxOutlineNode {
                    id: 4,
                    role: "AXButton".into(),
                    title: Some("Continue".into()),
                    enabled: Some(true),
                    ..AxOutlineNode::default()
                },
            ],
            ..AxOutlineNode::default()
        }];
        let rendered = render_ax_outline("Safari", &tree, false);
        assert_eq!(
            rendered,
            "\
Application: Safari

[1] AXWindow \"Checkout\"
  [2] AXTextField \"Email\"
      value=\"\"
  [3] AXTextField \"Company\"
      value=\"\"
  [4] AXButton \"Continue\"
      enabled=true"
        );
    }

    #[test]
    fn truncates_long_values_and_marks_budget() {
        let long = "a".repeat(MAX_AX_TEXT_CHARS + 8);
        let tree = vec![AxOutlineNode {
            id: 1,
            role: "AXWebArea".into(),
            value: Some(truncate_ax_text(&long)),
            truncated_children: true,
            ..AxOutlineNode::default()
        }];
        let rendered = render_ax_outline("Safari", &tree, true);
        assert!(rendered.contains(" (truncated)"));
        assert!(rendered.contains("… truncated"));
        assert!(rendered.contains('…'));
        assert!(!rendered.contains(&long));
    }
}
