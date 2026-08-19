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
    /// Ask the user for a username/password. Values must not appear here.
    CredentialRequired {
        task_id: TaskId,
        approval_id: ApprovalId,
        title: String,
        credential_ref: String,
        save_offered: bool,
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
    fn credential_required_serializes_labels_only() {
        let event = AgentEvent::CredentialRequired {
            task_id: TaskId::new(),
            approval_id: crate::command::ApprovalId::new(),
            title: "Enter login for lab.fileserver".into(),
            credential_ref: "lab.fileserver".into(),
            save_offered: true,
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "credential_required");
        assert_eq!(json["credential_ref"], "lab.fileserver");
        assert_eq!(json["save_offered"].as_bool(), Some(true));
        assert!(json.get("username").is_none());
        assert!(json.get("password").is_none());
        let text = json.to_string();
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("labuser"));
    }
}
