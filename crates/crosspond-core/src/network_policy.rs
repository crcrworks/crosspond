use serde_json::Value;

const MAX_PRIVATE_VALUES: usize = 64;
const MIN_PRIVATE_VALUE_CHARS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolEgress {
    None,
    External,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolOutputPrivacy {
    Public,
    Private,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolSecurityMetadata {
    pub egress: ToolEgress,
    pub output_privacy: ToolOutputPrivacy,
}

fn has_credential_ref(input: &Value) -> bool {
    input
        .get("credential_ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

/// Fail-closed: unknown tools are treated as private local data that may leave the machine.
pub fn security_metadata(name: &str, input: &Value) -> ToolSecurityMetadata {
    match name {
        "web_search" => ToolSecurityMetadata {
            egress: ToolEgress::External,
            output_privacy: ToolOutputPrivacy::Public,
        },
        "fetch_url" => ToolSecurityMetadata {
            egress: ToolEgress::External,
            output_privacy: if has_credential_ref(input) {
                ToolOutputPrivacy::Private
            } else {
                ToolOutputPrivacy::Public
            },
        },
        "open_url" | "browser_navigate" | "browser_new_tab" | "browser_fill" | "browser_type" => {
            ToolSecurityMetadata {
                egress: ToolEgress::External,
                output_privacy: if matches!(name, "open_url") {
                    ToolOutputPrivacy::Public
                } else {
                    ToolOutputPrivacy::Private
                },
            }
        }
        "read_file"
        | "list_directory"
        | "write_file"
        | "create_directory"
        | "run_command"
        | "calendar_events"
        | "knowledge_search"
        | "knowledge_read"
        | "knowledge_neighbors"
        | "knowledge_backlinks"
        | "knowledge_find_procedure"
        | "knowledge_ingest"
        | "knowledge_propose_update"
        | "knowledge_read_later"
        | "knowledge_archive_source"
        | "browser_tabs"
        | "browser_snapshot"
        | "browser_text"
        | "browser_click"
        | "browser_press_key"
        | "browser_scroll"
        | "browser_select"
        | "take_screenshot"
        | "get_accessibility_snapshot"
        | "list_apps"
        | "open_app"
        | "focus_app"
        | "ui_press"
        | "ui_set_value"
        | "ui_click"
        | "ui_type"
        | "ui_hotkey"
        | "ui_scroll"
        | "fill_credential" => ToolSecurityMetadata {
            egress: ToolEgress::None,
            output_privacy: ToolOutputPrivacy::Private,
        },
        _ => ToolSecurityMetadata {
            egress: ToolEgress::External,
            output_privacy: ToolOutputPrivacy::Private,
        },
    }
}

pub fn is_egress_tool(name: &str, input: &Value) -> bool {
    security_metadata(name, input).egress == ToolEgress::External
}

pub fn output_is_private(name: &str, input: &Value) -> bool {
    security_metadata(name, input).output_privacy == ToolOutputPrivacy::Private
}

pub fn remember_private_value(values: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.chars().count() < MIN_PRIVATE_VALUE_CHARS {
        return;
    }
    if values.iter().any(|existing| existing == trimmed) {
        return;
    }
    values.push(trimmed.to_string());
    if values.len() > MAX_PRIVATE_VALUES {
        values.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_fetch_url_is_egress() {
        assert!(is_egress_tool(
            "fetch_url",
            &json!({"url": "https://files.example.invalid/", "credential_ref": "lab"})
        ));
        assert!(is_egress_tool(
            "fetch_url",
            &json!({"url": "https://example.com/"})
        ));
        assert!(is_egress_tool("web_search", &json!({"query": "rust 1.96"})));
    }

    #[test]
    fn authenticated_fetch_output_is_private() {
        assert_eq!(
            security_metadata(
                "fetch_url",
                &json!({"url": "https://files.example.invalid/", "credential_ref": "lab"})
            )
            .output_privacy,
            ToolOutputPrivacy::Private
        );
        assert_eq!(
            security_metadata("fetch_url", &json!({"url": "https://example.com/"})).output_privacy,
            ToolOutputPrivacy::Public
        );
        assert!(output_is_private(
            "run_command",
            &json!({"command": "printf hi"})
        ));
        assert!(output_is_private("browser_tabs", &json!({})));
        assert!(output_is_private("list_directory", &json!({"path": "."})));
    }

    #[test]
    fn unknown_tools_default_private_and_egress() {
        let meta = security_metadata("future_exfil", &json!({}));
        assert_eq!(meta.egress, ToolEgress::External);
        assert_eq!(meta.output_privacy, ToolOutputPrivacy::Private);
    }

    #[test]
    fn skips_tiny_private_values() {
        let mut values = Vec::new();
        remember_private_value(&mut values, "ab");
        remember_private_value(&mut values, "  secret text  ");
        remember_private_value(&mut values, "secret text");
        assert_eq!(values, ["secret text"]);
    }
}
