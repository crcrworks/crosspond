const PLACEHOLDER: &str = "[redacted]";

/// Replace known raw private values with a placeholder. Paraphrases are left as-is.
pub fn redact_known_values(text: &str, values: &[String]) -> String {
    if text.is_empty() || values.is_empty() {
        return text.to_string();
    }
    let mut ranked: Vec<&str> = values
        .iter()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    ranked.sort_by_key(|value| std::cmp::Reverse(value.len()));
    let mut redacted = text.to_string();
    for value in ranked {
        if value.is_empty() || !redacted.contains(value) {
            continue;
        }
        redacted = redacted.replace(value, PLACEHOLDER);
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_longest_match_first() {
        let text = "The secret document contents were quoted.";
        let out = redact_known_values(text, &["secret document contents".into(), "secret".into()]);
        assert_eq!(out, "The [redacted] were quoted.");
        assert!(!out.contains("secret document contents"));
    }

    #[test]
    fn leaves_paraphrase_alone() {
        let out = redact_known_values(
            "The user selected some private notes.",
            &["classified lab protocol 7".into()],
        );
        assert_eq!(out, "The user selected some private notes.");
    }
}
