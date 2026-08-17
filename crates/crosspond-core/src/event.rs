use std::path::PathBuf;

use crate::ids::TaskId;
use crate::receipt::Receipt;

use super::command::ApprovalId;

/// Runtime → UI.
///
/// Phase 12 adds a `Receipt` on `TaskCompleted` and a filesystem `path` on
/// `ArtifactCreated` so the launcher can reveal files in Finder.
#[derive(Clone, Debug)]
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
