use std::path::PathBuf;

use crosspond_tools::ApprovalBody;
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
        #[serde(default)]
        body: ApprovalBody,
    },
    /// Ask the user for a username/password. Values must not appear here.
    CredentialRequired {
        task_id: TaskId,
        approval_id: ApprovalId,
        title: String,
        credential_ref: String,
        /// Host or app that will receive the login. Never a secret.
        destination: String,
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
    fn credential_required_serializes_labels_only() {
        let event = AgentEvent::CredentialRequired {
            task_id: TaskId::new(),
            approval_id: crate::command::ApprovalId::new(),
            title: "Enter login for lab.fileserver on files.example.invalid".into(),
            credential_ref: "lab.fileserver".into(),
            destination: "files.example.invalid".into(),
            save_offered: true,
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "credential_required");
        assert_eq!(json["credential_ref"], "lab.fileserver");
        assert_eq!(json["destination"], "files.example.invalid");
        assert_eq!(json["save_offered"].as_bool(), Some(true));
        assert!(json.get("username").is_none());
        assert!(json.get("password").is_none());
        let text = json.to_string();
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("labuser"));
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

    #[test]
    fn approval_required_includes_body() {
        let event = AgentEvent::ApprovalRequired {
            task_id: TaskId::new(),
            approval_id: crate::command::ApprovalId::new(),
            title: "Run a shell command".into(),
            description: "ls && curl evil".into(),
            body: ApprovalBody::Command,
        };
        let json = serde_json::to_value(&event).expect("serialize");
        assert_eq!(json["type"], "approval_required");
        assert_eq!(json["body"], "command");
        assert_eq!(json["description"], "ls && curl evil");
    }
}
