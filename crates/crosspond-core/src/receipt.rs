use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::TaskId;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub task_id: String,
    pub summary: String,
    pub actions: Vec<String>,
    pub artifacts: Vec<String>,
}

pub fn write_receipt(task_dir: &Path, receipt: &Receipt) -> Result<(), String> {
    fs::create_dir_all(task_dir).map_err(|err| err.to_string())?;
    let json = serde_json::to_string_pretty(receipt).map_err(|err| err.to_string())?;
    fs::write(task_dir.join("receipt.json"), json).map_err(|err| err.to_string())
}

pub fn write_task_meta(
    task_dir: &Path,
    task_id: TaskId,
    prompt: &str,
    status: &str,
    workspace: Option<&Path>,
) {
    let _ = fs::create_dir_all(task_dir);
    let json = serde_json::json!({
        "id": task_id.to_string(),
        "prompt": prompt,
        "status": status,
        "workspace": workspace.map(|path| path.to_string_lossy().into_owned()),
    });
    if let Ok(text) = serde_json::to_string_pretty(&json) {
        let _ = fs::write(task_dir.join("task.json"), text);
    }
}

/// Append a structured event. Do not include secrets or full prompt text.
pub fn append_event_log(task_dir: &Path, event: Value) {
    let _ = fs::create_dir_all(task_dir);
    let Ok(mut line) = serde_json::to_string(&event) else {
        return;
    };
    line.push('\n');
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(task_dir.join("events.jsonl"))
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// User-facing receipt line for a successful tool call.
///
/// Omits file bodies, command output, calendar notes, URL query strings,
/// typed text, and field values.
pub fn receipt_action_line(name: &str, text: &str) -> Option<String> {
    match name {
        "write_file" | "create_directory" => first_line(text),
        "ui_press" | "ui_click" | "ui_hotkey" | "ui_scroll" | "open_app" | "focus_app" => {
            first_line(text)
        }
        "ui_type" => Some("Typed text".into()),
        "ui_set_value" => Some("Set a field value".into()),
        "run_command" => Some("Ran a command".into()),
        "open_url" => Some("Opened a URL".into()),
        "calendar_events" => Some("Read calendar events".into()),
        _ => None,
    }
}

const UI_SUMMARY_MAX: usize = 120;

/// One-line tool detail for the command-window transcript.
///
/// Built from arguments only. Never includes command output, typed text,
/// field values, calendar notes, or URL query strings.
pub fn tool_ui_summary(name: &str, input: &Value) -> String {
    match name {
        "read_file" | "write_file" | "list_directory" | "create_directory" => {
            path_basename(string_field(input, "path"))
        }
        "run_command" => truncate_chars(&string_field(input, "command"), UI_SUMMARY_MAX),
        "web_search" | "knowledge_search" | "knowledge_find_procedure" => {
            truncate_chars(&string_field(input, "query"), UI_SUMMARY_MAX)
        }
        "fetch_url" | "open_url" => url_without_query(string_field(input, "url")),
        "knowledge_read"
        | "knowledge_neighbors"
        | "knowledge_backlinks"
        | "knowledge_propose_update"
        | "knowledge_archive_source" => truncate_chars(&string_field(input, "id"), UI_SUMMARY_MAX),
        "knowledge_ingest" | "knowledge_read_later" => {
            truncate_chars(&string_field(input, "title"), UI_SUMMARY_MAX)
        }
        "open_app" | "focus_app" => {
            let name = string_field(input, "name");
            if !name.is_empty() {
                name
            } else {
                string_field(input, "bundle_id")
            }
        }
        "get_accessibility_snapshot" | "take_screenshot" => string_field(input, "app"),
        "calendar_events" => calendar_range(input),
        "ui_hotkey" => keys_summary(input),
        "ui_type" | "ui_set_value" | "ui_press" | "ui_click" | "ui_scroll" => String::new(),
        _ => String::new(),
    }
}

fn string_field(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

fn path_basename(path: String) -> String {
    if path.is_empty() {
        return path;
    }
    Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

fn url_without_query(url: String) -> String {
    if url.is_empty() {
        return url;
    }
    let stripped = url.split(['?', '#']).next().unwrap_or(&url);
    truncate_chars(stripped, UI_SUMMARY_MAX)
}

fn calendar_range(input: &Value) -> String {
    let start = string_field(input, "start");
    let end = string_field(input, "end");
    match (start.is_empty(), end.is_empty()) {
        (true, true) => String::new(),
        (false, true) => start,
        (true, false) => end,
        (false, false) => format!("{start}–{end}"),
    }
}

fn keys_summary(input: &Value) -> String {
    let Some(keys) = input.get("keys").and_then(Value::as_array) else {
        return String::new();
    };
    let joined = keys
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>()
        .join("+");
    truncate_chars(&joined, UI_SUMMARY_MAX)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let short: String = text.chars().take(max).collect();
        format!("{short}…")
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_omits_sensitive_tool_bodies() {
        assert_eq!(
            receipt_action_line("run_command", "stdout secret"),
            Some("Ran a command".into())
        );
        assert_eq!(
            receipt_action_line("open_url", "Opened https://example.com/?token=abc"),
            Some("Opened a URL".into())
        );
        assert_eq!(
            receipt_action_line("calendar_events", "Meeting\nnotes: confidential"),
            Some("Read calendar events".into())
        );
        assert_eq!(
            receipt_action_line("ui_type", "Typed hunter2"),
            Some("Typed text".into())
        );
        assert_eq!(
            receipt_action_line("write_file", "Wrote output/hello.txt"),
            Some("Wrote output/hello.txt".into())
        );
        assert_eq!(receipt_action_line("read_file", "file body"), None);
    }

    #[test]
    fn tool_ui_summary_omits_secrets_and_query_strings() {
        assert_eq!(
            tool_ui_summary("ui_type", &serde_json::json!({"text": "hunter2"})),
            ""
        );
        assert_eq!(
            tool_ui_summary("ui_set_value", &serde_json::json!({"value": "secret"})),
            ""
        );
        assert_eq!(
            tool_ui_summary(
                "open_url",
                &serde_json::json!({"url": "https://example.com/path?token=abc"})
            ),
            "https://example.com/path"
        );
        assert_eq!(
            tool_ui_summary(
                "fetch_url",
                &serde_json::json!({"url": "https://example.com/x#frag"})
            ),
            "https://example.com/x"
        );
        assert_eq!(
            tool_ui_summary(
                "calendar_events",
                &serde_json::json!({
                    "start": "2026-08-18",
                    "end": "2026-08-19",
                    "notes": "confidential"
                })
            ),
            "2026-08-18–2026-08-19"
        );
        assert_eq!(
            tool_ui_summary("run_command", &serde_json::json!({"command": "ls -la"})),
            "ls -la"
        );
        assert_eq!(
            tool_ui_summary(
                "read_file",
                &serde_json::json!({"path": "/Users/me/Downloads/video.mov"})
            ),
            "video.mov"
        );
        assert_eq!(
            tool_ui_summary("web_search", &serde_json::json!({"query": "rust gpui"})),
            "rust gpui"
        );
        assert_eq!(
            tool_ui_summary("ui_hotkey", &serde_json::json!({"keys": ["cmd", "c"]})),
            "cmd+c"
        );
    }
}
