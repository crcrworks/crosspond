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
}
