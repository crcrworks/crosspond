const MAX_PRIVATE_VALUES: usize = 64;
const MIN_PRIVATE_VALUE_CHARS: usize = 4;

pub fn is_private_source_tool(name: &str) -> bool {
    matches!(
        name,
        "calendar_events"
            | "knowledge_read"
            | "knowledge_search"
            | "knowledge_neighbors"
            | "knowledge_backlinks"
            | "knowledge_find_procedure"
            | "read_file"
            | "get_accessibility_snapshot"
            | "take_screenshot"
            | "browser_snapshot"
            | "browser_text"
    )
}

pub fn is_egress_tool(name: &str, input: &serde_json::Value) -> bool {
    match name {
        "web_search" | "open_url" | "browser_navigate" | "browser_new_tab" | "browser_fill"
        | "browser_type" => true,
        "fetch_url" => input
            .get("credential_ref")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_none_or(|value| value.is_empty()),
        _ => false,
    }
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
    fn fetch_url_with_credential_ref_is_not_egress() {
        assert!(!is_egress_tool(
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
    fn skips_tiny_private_values() {
        let mut values = Vec::new();
        remember_private_value(&mut values, "ab");
        remember_private_value(&mut values, "  secret text  ");
        remember_private_value(&mut values, "secret text");
        assert_eq!(values, ["secret text"]);
    }
}
