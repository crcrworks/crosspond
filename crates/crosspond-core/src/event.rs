use crate::ids::TaskId;

use super::command::ApprovalId;

/// Runtime → UI.
///
/// Phase 3 emits task lifecycle events, `ContextCollected`, `AssistantDelta`,
/// tool activity, `ArtifactCreated`, and `ConnectionTested`.
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
    ToolStarted {
        task_id: TaskId,
        tool: String,
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
    },
    TaskCompleted {
        task_id: TaskId,
        summary: String,
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
