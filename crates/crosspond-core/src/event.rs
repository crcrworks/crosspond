use std::path::PathBuf;

use serde::Serialize;

use crate::ids::TaskId;
use crate::receipt::Receipt;

use super::command::ApprovalId;

/// Runtime → UI.
///
/// Phase 12 adds a `Receipt` on `TaskCompleted` and a filesystem `path` on
/// `ArtifactCreated` so the launcher can reveal files in Finder.
/// `path` is skipped in JSON so the WebView never receives Finder paths.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TaskStarted {
        task_id: TaskId,
        prompt: String,
    },
    ContextCollected {
        task_id: TaskId,
    },
    AssistantDelta {
        task_id: TaskId,
        text: String,
    },
    ReasoningDelta {
        task_id: TaskId,
        text: String,
    },
    ToolStarted {
        task_id: TaskId,
        tool: String,
        /// UI-only; omit secrets, query strings, field values, and command output.
        summary: String,
    },
    ToolFinished {
        task_id: TaskId,
        tool: String,
    },
    ApprovalRequired {
        task_id: TaskId,
        approval_id: ApprovalId,
        title: String,
        description: String,
    },
    ArtifactCreated {
        task_id: TaskId,
        display_name: String,
        #[serde(skip)]
        path: PathBuf,
    },
    TaskCompleted {
        task_id: TaskId,
        summary: String,
        receipt: Receipt,
    },
    TaskFailed {
        task_id: TaskId,
        message: String,
    },
    TaskCancelled {
        task_id: TaskId,
    },
    ConnectionTested {
        source: String,
        ok: bool,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TaskId;

    #[test]
    fn artifact_path_is_not_serialized() {
        let event = AgentEvent::ArtifactCreated {
            task_id: TaskId::new(),
            display_name: "notes.md".into(),
            path: PathBuf::from("/Users/me/secret/notes.md"),
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "artifact_created");
        assert_eq!(json["display_name"], "notes.md");
        assert!(json.get("path").is_none());
    }

    #[test]
    fn connection_tested_includes_source() {
        let event = AgentEvent::ConnectionTested {
            source: "chatgpt".into(),
            ok: false,
            message: "rejected".into(),
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "connection_tested");
        assert_eq!(json["source"], "chatgpt");
        assert_eq!(json["ok"], false);
        assert_eq!(json["message"], "rejected");
    }
}
