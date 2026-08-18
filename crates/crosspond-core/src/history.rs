use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;

use crate::receipt::Receipt;

const DEFAULT_LIMIT: usize = 50;
const TITLE_CHARS: usize = 72;

#[derive(Clone, Debug, Deserialize)]
struct TaskMetaFile {
    id: String,
    prompt: String,
    status: String,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskHistoryEntry {
    pub id: String,
    pub prompt: String,
    pub status: String,
    pub workspace: Option<String>,
    pub modified: SystemTime,
    pub receipt: Option<Receipt>,
}

#[derive(Clone, Debug)]
pub(crate) struct TaskRecord {
    #[allow(dead_code)]
    pub id: String,
    pub conversation_id: String,
    pub prompt: String,
    pub status: String,
    pub workspace: Option<String>,
    pub modified: SystemTime,
    pub receipt: Option<Receipt>,
    pub dir: PathBuf,
}

impl TaskHistoryEntry {
    pub fn title(&self) -> String {
        history_title(&self.prompt)
    }

    pub fn status_mark(&self) -> &'static str {
        match self.status.as_str() {
            "completed" => "✓",
            "failed" => "✕",
            "cancelled" => "—",
            "running" => "…",
            _ => "·",
        }
    }

    pub fn artifact_path(&self, name: &str) -> Option<PathBuf> {
        artifact_path(self.workspace.as_deref(), name)
    }
}

pub fn list_recent_tasks(root: &Path, limit: usize) -> Vec<TaskHistoryEntry> {
    let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };
    let mut groups: HashMap<String, Vec<TaskRecord>> = HashMap::new();
    for task in load_all_tasks(root) {
        groups
            .entry(task.conversation_id.clone())
            .or_default()
            .push(task);
    }
    let mut entries: Vec<TaskHistoryEntry> = groups
        .into_values()
        .filter_map(|mut tasks| {
            tasks.sort_by_key(|task| task.modified);
            let first = tasks.first()?;
            let last = tasks.last()?;
            Some(TaskHistoryEntry {
                id: last.conversation_id.clone(),
                prompt: first.prompt.clone(),
                status: last.status.clone(),
                workspace: last.workspace.clone(),
                modified: last.modified,
                receipt: last.receipt.clone(),
            })
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.modified));
    entries.truncate(limit);
    entries
}

pub fn history_title(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let mut title: String = line.chars().take(TITLE_CHARS).collect();
    if line.chars().count() > TITLE_CHARS {
        title.push('…');
    }
    if title.is_empty() {
        "Untitled".into()
    } else {
        title
    }
}

pub fn history_group_label(modified: SystemTime, now: SystemTime) -> &'static str {
    let Ok(elapsed) = now.duration_since(modified) else {
        return "Today";
    };
    match elapsed.as_secs() {
        0..=86_400 => "Today",
        86_401..=172_800 => "Yesterday",
        172_801..=604_800 => "This week",
        _ => "Earlier",
    }
}

pub(crate) fn load_all_tasks(root: &Path) -> Vec<TaskRecord> {
    let Ok(read) = fs::read_dir(root) else {
        return Vec::new();
    };
    read.flatten()
        .filter_map(|child| load_task_record(&child.path()))
        .collect()
}

pub(crate) fn tasks_for_conversation(root: &Path, conversation_id: &str) -> Vec<TaskRecord> {
    let mut tasks: Vec<TaskRecord> = load_all_tasks(root)
        .into_iter()
        .filter(|task| task.conversation_id == conversation_id)
        .collect();
    tasks.sort_by_key(|task| task.modified);
    tasks
}

fn load_task_record(dir: &Path) -> Option<TaskRecord> {
    if !dir.is_dir() {
        return None;
    }
    let meta_path = dir.join("task.json");
    let meta_text = fs::read_to_string(&meta_path).ok()?;
    let meta: TaskMetaFile = serde_json::from_str(&meta_text).ok()?;
    if meta.id.is_empty() {
        return None;
    }
    let modified = fs::metadata(&meta_path)
        .and_then(|info| info.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let receipt = fs::read_to_string(dir.join("receipt.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let conversation_id = meta
        .conversation_id
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| meta.id.clone());
    Some(TaskRecord {
        conversation_id,
        id: meta.id,
        prompt: meta.prompt,
        status: meta.status,
        workspace: meta.workspace,
        modified,
        receipt,
        dir: dir.to_path_buf(),
    })
}

pub(crate) fn artifact_path(workspace: Option<&str>, name: &str) -> Option<PathBuf> {
    let workspace = PathBuf::from(workspace?);
    let relative = Path::new(name);
    if name.is_empty() || relative.is_absolute() {
        return None;
    }
    if relative
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(workspace.join("output").join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ConversationId, TaskId};
    use crate::receipt::{write_receipt, write_task_meta};

    #[test]
    fn lists_recent_tasks_newest_first() {
        let root = std::env::temp_dir().join(format!("crosspond-history-{}", uuid::Uuid::new_v4()));
        let older = TaskId::new();
        let newer = TaskId::new();
        write_task_meta(
            &root.join(older.to_string()),
            older,
            "older prompt",
            "completed",
            None,
            ConversationId::new(),
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_task_meta(
            &root.join(newer.to_string()),
            newer,
            "newer prompt",
            "failed",
            Some(Path::new("/tmp/ws")),
            ConversationId::new(),
        );
        let _ = write_receipt(
            &root.join(newer.to_string()),
            &Receipt {
                task_id: newer.to_string(),
                summary: "done".into(),
                actions: vec!["Wrote output/a.txt".into()],
                artifacts: vec!["a.txt".into()],
            },
        );

        let listed = list_recent_tasks(&root, 10);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title(), "newer prompt");
        assert_eq!(listed[0].status_mark(), "✕");
        assert_eq!(listed[0].receipt.as_ref().unwrap().artifacts, ["a.txt"]);
        assert_eq!(listed[1].title(), "older prompt");
        assert_eq!(listed[1].status_mark(), "✓");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn groups_follow_ups_by_conversation() {
        let root =
            std::env::temp_dir().join(format!("crosspond-history-group-{}", uuid::Uuid::new_v4()));
        let conversation = ConversationId::new();
        let first = TaskId::new();
        let second = TaskId::new();
        write_task_meta(
            &root.join(first.to_string()),
            first,
            "first prompt",
            "completed",
            None,
            conversation,
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_task_meta(
            &root.join(second.to_string()),
            second,
            "follow-up",
            "failed",
            Some(Path::new("/tmp/ws")),
            conversation,
        );
        let _ = write_receipt(
            &root.join(second.to_string()),
            &Receipt {
                task_id: second.to_string(),
                summary: "later".into(),
                actions: Vec::new(),
                artifacts: vec!["b.txt".into()],
            },
        );
        let listed = list_recent_tasks(&root, 10);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, conversation.to_string());
        assert_eq!(listed[0].title(), "first prompt");
        assert_eq!(listed[0].status, "failed");
        assert_eq!(listed[0].receipt.as_ref().unwrap().artifacts, ["b.txt"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_tasks_without_conversation_id_stay_separate() {
        let root =
            std::env::temp_dir().join(format!("crosspond-history-legacy-{}", uuid::Uuid::new_v4()));
        let first = TaskId::new();
        let second = TaskId::new();
        let write_legacy = |id: TaskId, prompt: &str| {
            let dir = root.join(id.to_string());
            fs::create_dir_all(&dir).unwrap();
            let json = serde_json::json!({
                "id": id.to_string(),
                "prompt": prompt,
                "status": "completed",
                "workspace": null,
            });
            fs::write(
                dir.join("task.json"),
                serde_json::to_string_pretty(&json).unwrap(),
            )
            .unwrap();
        };
        write_legacy(first, "alpha");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_legacy(second, "beta");
        let listed = list_recent_tasks(&root, 10);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.to_string());
        assert_eq!(listed[1].id, first.to_string());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn history_title_truncates_and_skips_blank_lines() {
        assert_eq!(history_title("\n  Hello world  \n"), "Hello world");
        assert_eq!(history_title(""), "Untitled");
        let long = "a".repeat(80);
        let title = history_title(&long);
        assert!(title.ends_with('…'));
        assert_eq!(title.chars().count(), TITLE_CHARS + 1);
    }

    #[test]
    fn artifact_path_stays_inside_workspace() {
        let entry = TaskHistoryEntry {
            id: "t".into(),
            prompt: "p".into(),
            status: "completed".into(),
            workspace: Some("/tmp/ws".into()),
            modified: SystemTime::UNIX_EPOCH,
            receipt: None,
        };
        assert_eq!(
            entry.artifact_path("hello.txt"),
            Some(PathBuf::from("/tmp/ws/output/hello.txt"))
        );
        assert_eq!(entry.artifact_path("../escape.txt"), None);
        assert_eq!(entry.artifact_path("/etc/passwd"), None);
    }

    #[test]
    fn groups_by_age() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        assert_eq!(
            history_group_label(now - std::time::Duration::from_secs(10), now),
            "Today"
        );
        assert_eq!(
            history_group_label(now - std::time::Duration::from_secs(90_000), now),
            "Yesterday"
        );
        assert_eq!(
            history_group_label(now - std::time::Duration::from_secs(400_000), now),
            "This week"
        );
        assert_eq!(
            history_group_label(now - std::time::Duration::from_secs(900_000), now),
            "Earlier"
        );
    }
}
