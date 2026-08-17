use crate::context::ContextCapsule;
use crate::ids::TaskId;

/// Identifier for a pending approval prompt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalId(uuid::Uuid);

impl ApprovalId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for ApprovalId {
    fn default() -> Self {
        Self::new()
    }
}

/// UI → runtime.
#[derive(Clone, Debug)]
pub enum RuntimeCommand {
    StartTask(StartTaskRequest),
    Approve(ApprovalId),
    Reject(ApprovalId),
    Cancel(TaskId),
    /// Drop in-memory follow-up history. Sent when the user starts a new conversation (New).
    ResetSession,
    /// Verify the current provider settings without starting a task.
    TestConnection,
}

#[derive(Clone, Debug)]
pub struct StartTaskRequest {
    pub task_id: TaskId,
    pub prompt: String,
    pub context: ContextCapsule,
}

impl StartTaskRequest {
    pub fn new(task_id: TaskId, prompt: impl Into<String>) -> Self {
        Self {
            task_id,
            prompt: prompt.into(),
            context: ContextCapsule::default(),
        }
    }
}
