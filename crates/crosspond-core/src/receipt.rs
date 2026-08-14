use std::fs;
use std::io::Write;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::ids::TaskId;

#[derive(Clone, Debug, Serialize)]
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

pub fn write_task_meta(task_dir: &Path, task_id: TaskId, prompt: &str, status: &str) {
    let _ = fs::create_dir_all(task_dir);
    let json = serde_json::json!({
        "id": task_id.to_string(),
        "prompt": prompt,
        "status": status,
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
