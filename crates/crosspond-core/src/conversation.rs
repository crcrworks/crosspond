use std::fs;
use std::path::{Path, PathBuf};

use crosspond_model::{Message, Role, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::history::{TaskRecord, tasks_for_conversation};
use crate::receipt::{Receipt, tool_ui_summary};

const TOOL_PLACEHOLDER: &str = "Tool finished.";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TranscriptBlock {
    User {
        text: String,
    },
    Work {
        steps: Vec<WorkStep>,
        expanded: bool,
        #[serde(rename = "startedAt")]
        started_at: u64,
        #[serde(rename = "workedMs")]
        worked_ms: Option<u64>,
    },
    Text {
        text: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkStep {
    Thinking {
        text: String,
        expanded: bool,
        #[serde(rename = "startedAt")]
        started_at: u64,
        #[serde(rename = "durationMs")]
        duration_ms: Option<u64>,
    },
    Tool {
        tool: ToolLine,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolLine {
    pub name: String,
    pub summary: String,
    pub running: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PersistedMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<PersistedToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PersistedToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConversationView {
    pub id: String,
    pub status: String,
    pub transcript: Vec<TranscriptBlock>,
    pub receipt: Option<Receipt>,
    pub artifact_names: Vec<String>,
}

pub fn write_session(task_dir: &Path, messages: &[Message]) {
    let persisted = sanitize_messages(messages);
    if persisted.is_empty() {
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(&persisted) else {
        return;
    };
    let _ = fs::create_dir_all(task_dir);
    let _ = fs::write(task_dir.join("session.json"), json);
}

pub fn sanitize_messages(messages: &[Message]) -> Vec<PersistedMessage> {
    messages
        .iter()
        .filter_map(|message| match message.role {
            Role::System => None,
            Role::User => Some(PersistedMessage {
                role: "user".into(),
                content: message.content.clone(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }),
            Role::Assistant => Some(PersistedMessage {
                role: "assistant".into(),
                content: message.content.clone(),
                tool_calls: message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        let input =
                            serde_json::from_str(&call.arguments).unwrap_or_else(|_| json!({}));
                        PersistedToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            summary: tool_ui_summary(&call.name, &input),
                        }
                    })
                    .collect(),
                tool_call_id: None,
            }),
            Role::Tool => Some(PersistedMessage {
                role: "tool".into(),
                content: TOOL_PLACEHOLDER.into(),
                tool_calls: Vec::new(),
                tool_call_id: message.tool_call_id.clone(),
            }),
        })
        .collect()
}

pub fn restore_messages(persisted: &[PersistedMessage]) -> Vec<Message> {
    persisted
        .iter()
        .filter_map(|message| match message.role.as_str() {
            "user" => Some(Message::user(message.content.clone())),
            "assistant" if message.tool_calls.is_empty() => {
                Some(Message::assistant(message.content.clone()))
            }
            "assistant" => Some(Message::assistant_tool_calls(
                message.content.clone(),
                message
                    .tool_calls
                    .iter()
                    .map(|call| ToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: if call.summary.is_empty() {
                            "{}".into()
                        } else {
                            json!({ "summary": call.summary }).to_string()
                        },
                    })
                    .collect(),
            )),
            "tool" => Some(Message::tool(
                message.tool_call_id.clone().unwrap_or_default(),
                TOOL_PLACEHOLDER,
            )),
            _ => None,
        })
        .collect()
}

pub fn load_session_messages(root: &Path, conversation_id: &str) -> Vec<Message> {
    let tasks = tasks_for_conversation(root, conversation_id);
    for task in tasks.iter().rev() {
        if let Some(messages) = read_session(&task.dir) {
            return messages;
        }
    }
    reconstruct_session(&tasks)
}

fn read_session(task_dir: &Path) -> Option<Vec<Message>> {
    let text = fs::read_to_string(task_dir.join("session.json")).ok()?;
    let persisted: Vec<PersistedMessage> = serde_json::from_str(&text).ok()?;
    Some(restore_messages(&persisted))
}

fn reconstruct_session(tasks: &[TaskRecord]) -> Vec<Message> {
    let mut messages = Vec::new();
    for task in tasks {
        if !task.prompt.trim().is_empty() {
            messages.push(Message::user(task.prompt.clone()));
        }
        if let Some(summary) = task
            .receipt
            .as_ref()
            .map(|receipt| receipt.summary.trim())
            .filter(|summary| !summary.is_empty())
        {
            messages.push(Message::assistant(summary.to_string()));
        }
    }
    messages
}

pub fn open_conversation(root: &Path, conversation_id: &str) -> Option<ConversationView> {
    let tasks = tasks_for_conversation(root, conversation_id);
    if tasks.is_empty() {
        return None;
    }
    let mut transcript = Vec::new();
    for task in &tasks {
        transcript.extend(replay_task(task));
    }
    let latest = tasks.last()?;
    let artifact_names = latest
        .receipt
        .as_ref()
        .map(|receipt| receipt.artifacts.clone())
        .unwrap_or_default();
    Some(ConversationView {
        id: conversation_id.to_string(),
        status: latest.status.clone(),
        transcript,
        receipt: latest.receipt.clone(),
        artifact_names,
    })
}

pub fn conversation_artifact_path(
    root: &Path,
    conversation_id: &str,
    name: &str,
) -> Option<PathBuf> {
    let tasks = tasks_for_conversation(root, conversation_id);
    let latest = tasks.last()?;
    crate::history::artifact_path(latest.workspace.as_deref(), name)
}

pub fn replay_task(task: &TaskRecord) -> Vec<TranscriptBlock> {
    let events = read_event_log(&task.dir);
    replay_events(&task.prompt, &events, task.receipt.as_ref(), &task.status)
}

fn read_event_log(task_dir: &Path) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(task_dir.join("events.jsonl")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub fn replay_events(
    prompt: &str,
    events: &[Value],
    receipt: Option<&Receipt>,
    status: &str,
) -> Vec<TranscriptBlock> {
    let mut replay = Replay::default();
    if !prompt.trim().is_empty() {
        replay.push_user(prompt);
    }
    for event in events {
        let Some(kind) = event.get("type").and_then(Value::as_str) else {
            continue;
        };
        match kind {
            "reasoning" => {
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                let duration = event.get("duration_ms").and_then(Value::as_u64);
                replay.push_reasoning(text, duration);
            }
            "assistant_text" => {
                let text = event.get("text").and_then(Value::as_str).unwrap_or("");
                replay.push_text(text);
            }
            "tool_started" => {
                let name = event.get("tool").and_then(Value::as_str).unwrap_or("");
                let summary = event.get("summary").and_then(Value::as_str).unwrap_or("");
                replay.start_tool(name, summary);
            }
            "tool_finished" => {
                let name = event.get("tool").and_then(Value::as_str).unwrap_or("");
                let duration = event.get("duration_ms").and_then(Value::as_u64);
                replay.finish_tool(name, duration);
            }
            "task_failed" => {
                let message = event.get("message").and_then(Value::as_str).unwrap_or("");
                replay.push_notice(message);
            }
            "task_cancelled" => replay.seal_work(),
            _ => {}
        }
    }
    if !replay.has_assistant_text_since_last_user() {
        if let Some(summary) = receipt
            .map(|receipt| receipt.summary.trim())
            .filter(|summary| !summary.is_empty())
        {
            replay.push_text(summary);
        } else if status == "failed" {
            replay.push_notice("This task did not finish.");
        }
    }
    replay.seal_work();
    replay.blocks
}

#[derive(Default)]
struct Replay {
    blocks: Vec<TranscriptBlock>,
    work_ms: u64,
}

impl Replay {
    fn push_user(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        self.seal_work();
        self.blocks.push(TranscriptBlock::User {
            text: trimmed.to_string(),
        });
    }

    fn push_reasoning(&mut self, text: &str, duration_ms: Option<u64>) {
        if text.is_empty() {
            return;
        }
        self.ensure_work();
        let Some(TranscriptBlock::Work { steps, .. }) = self.blocks.last_mut() else {
            return;
        };
        freeze_thinking(steps);
        if let Some(ms) = duration_ms {
            self.work_ms = self.work_ms.saturating_add(ms);
        }
        steps.push(WorkStep::Thinking {
            text: text.to_string(),
            expanded: false,
            started_at: 0,
            duration_ms,
        });
    }

    fn start_tool(&mut self, name: &str, summary: &str) {
        if name.is_empty() {
            return;
        }
        self.ensure_work();
        let Some(TranscriptBlock::Work { steps, .. }) = self.blocks.last_mut() else {
            return;
        };
        freeze_thinking(steps);
        steps.push(WorkStep::Tool {
            tool: ToolLine {
                name: name.to_string(),
                summary: summary.to_string(),
                running: true,
            },
        });
    }

    fn finish_tool(&mut self, name: &str, duration_ms: Option<u64>) {
        if let Some(ms) = duration_ms {
            self.work_ms = self.work_ms.saturating_add(ms);
        }
        if let Some(TranscriptBlock::Work { steps, .. }) = self.blocks.last_mut()
            && let Some(WorkStep::Tool { tool }) = steps.iter_mut().rev().find(|step| {
                matches!(step, WorkStep::Tool { tool } if tool.running && (name.is_empty() || tool.name == name))
            })
        {
            tool.running = false;
            return;
        }
        if name.is_empty() {
            return;
        }
        self.start_tool(name, "");
        if let Some(TranscriptBlock::Work { steps, .. }) = self.blocks.last_mut()
            && let Some(WorkStep::Tool { tool }) = steps.last_mut()
        {
            tool.running = false;
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.seal_work();
        let trimmed = text.trim_start();
        if trimmed.is_empty() {
            return;
        }
        if let Some(TranscriptBlock::Text { text: existing }) = self.blocks.last_mut() {
            existing.push_str(trimmed);
            return;
        }
        self.blocks.push(TranscriptBlock::Text {
            text: trimmed.to_string(),
        });
    }

    fn push_notice(&mut self, message: &str) {
        if message.is_empty() {
            return;
        }
        self.seal_work();
        self.blocks.push(TranscriptBlock::Text {
            text: message.to_string(),
        });
    }

    fn ensure_work(&mut self) {
        if matches!(self.blocks.last(), Some(TranscriptBlock::Work { worked_ms, .. }) if worked_ms.is_none())
        {
            return;
        }
        self.work_ms = 0;
        self.blocks.push(TranscriptBlock::Work {
            steps: Vec::new(),
            expanded: false,
            started_at: 0,
            worked_ms: None,
        });
    }

    fn seal_work(&mut self) {
        let ms = self.work_ms;
        let Some(TranscriptBlock::Work {
            steps,
            worked_ms,
            expanded,
            ..
        }) = self.blocks.last_mut()
        else {
            return;
        };
        if worked_ms.is_some() {
            return;
        }
        freeze_thinking(steps);
        for step in steps.iter_mut() {
            if let WorkStep::Tool { tool } = step {
                tool.running = false;
            }
        }
        *worked_ms = Some(ms);
        *expanded = false;
        self.work_ms = 0;
    }

    fn has_assistant_text_since_last_user(&self) -> bool {
        for block in self.blocks.iter().rev() {
            match block {
                TranscriptBlock::User { .. } => return false,
                TranscriptBlock::Text { text } if !text.trim().is_empty() => return true,
                _ => {}
            }
        }
        false
    }
}

fn freeze_thinking(steps: &mut [WorkStep]) {
    if let Some(WorkStep::Thinking { duration_ms, .. }) = steps.last_mut()
        && duration_ms.is_none()
    {
        *duration_ms = Some(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ConversationId, TaskId};
    use crate::receipt::{append_event_log, write_receipt, write_task_meta};
    use crosspond_model::ImagePart;

    #[test]
    fn sanitize_drops_tool_bodies_arguments_and_images() {
        let messages = vec![
            Message::user("summarize"),
            Message::assistant_tool_calls(
                "I'll type.",
                vec![ToolCall {
                    id: "c1".into(),
                    name: "ui_type".into(),
                    arguments: json!({ "text": "hunter2" }).to_string(),
                }],
            ),
            Message::tool_with_images(
                "c1",
                "Typed hunter2 into the password field",
                vec![ImagePart {
                    media_type: "image/png".into(),
                    bytes: b"PNGSECRET".to_vec(),
                    width: Some(10),
                    height: Some(10),
                }],
            ),
            Message::assistant("Done."),
        ];
        let persisted = sanitize_messages(&messages);
        let json = serde_json::to_string(&persisted).unwrap();
        assert!(!json.contains("hunter2"));
        assert!(!json.contains("PNGSECRET"));
        assert!(!json.contains("password"));
        assert_eq!(persisted[1].tool_calls[0].name, "ui_type");
        assert_eq!(persisted[1].tool_calls[0].summary, "");
        assert_eq!(persisted[2].content, TOOL_PLACEHOLDER);
        let restored = restore_messages(&persisted);
        assert!(restored.iter().all(|message| message.images.is_empty()));
        assert_eq!(restored[2].content, TOOL_PLACEHOLDER);
        assert!(!restored[1].tool_calls[0].arguments.contains("hunter2"));
    }

    #[test]
    fn replay_builds_work_and_text_blocks() {
        let events = vec![
            json!({"type": "reasoning", "text": "plan", "duration_ms": 1200}),
            json!({"type": "tool_started", "tool": "read_file", "summary": "notes.md"}),
            json!({"type": "tool_finished", "tool": "read_file", "duration_ms": 800, "success": true}),
            json!({"type": "assistant_text", "text": "I read it."}),
        ];
        let blocks = replay_events("hello", &events, None, "completed");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], TranscriptBlock::User { text } if text == "hello"));
        match &blocks[1] {
            TranscriptBlock::Work {
                steps,
                worked_ms,
                expanded,
                ..
            } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(*worked_ms, Some(2000));
                assert!(!*expanded);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&blocks[2], TranscriptBlock::Text { text } if text == "I read it."));
    }

    #[test]
    fn legacy_replay_uses_receipt_summary() {
        let receipt = Receipt {
            task_id: "t".into(),
            summary: "All set.".into(),
            actions: vec!["Wrote output/a.txt".into()],
            artifacts: vec!["a.txt".into()],
        };
        let blocks = replay_events(
            "do the thing",
            &[
                json!({"type": "task_started"}),
                json!({"type": "task_completed"}),
            ],
            Some(&receipt),
            "completed",
        );
        assert!(matches!(&blocks[0], TranscriptBlock::User { text } if text == "do the thing"));
        assert!(matches!(&blocks[1], TranscriptBlock::Text { text } if text == "All set."));
    }

    #[test]
    fn open_conversation_joins_turns() {
        let root = std::env::temp_dir().join(format!("crosspond-conv-{}", uuid::Uuid::new_v4()));
        let conversation = ConversationId::new();
        let first = TaskId::new();
        let second = TaskId::new();
        let first_dir = root.join(first.to_string());
        let second_dir = root.join(second.to_string());
        write_task_meta(&first_dir, first, "first", "completed", None, conversation);
        append_event_log(&first_dir, json!({"type": "assistant_text", "text": "Hi."}));
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_task_meta(
            &second_dir,
            second,
            "again",
            "completed",
            None,
            conversation,
        );
        append_event_log(
            &second_dir,
            json!({"type": "assistant_text", "text": "Sure."}),
        );
        let _ = write_receipt(
            &second_dir,
            &Receipt {
                task_id: second.to_string(),
                summary: "Sure.".into(),
                actions: Vec::new(),
                artifacts: vec!["out.txt".into()],
            },
        );
        let view = open_conversation(&root, &conversation.to_string()).unwrap();
        assert_eq!(view.id, conversation.to_string());
        assert_eq!(view.artifact_names, ["out.txt"]);
        assert_eq!(view.transcript.len(), 4);
        let _ = fs::remove_dir_all(root);
    }
}
