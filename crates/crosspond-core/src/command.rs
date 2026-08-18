use serde::{Deserialize, Serialize};

use crate::context::ContextCapsule;
use crate::ids::{ConversationId, TaskId};
use crate::mention::Mention;

/// Identifier for a pending approval prompt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
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
#[allow(clippy::large_enum_variant)] // StartTask carries context + mentions; keep match sites unboxed.
pub enum RuntimeCommand {
    StartTask(StartTaskRequest),
    Approve(ApprovalId),
    Reject(ApprovalId),
    Cancel(TaskId),
    /// Drop in-memory follow-up history. Sent when the user starts a new conversation (New).
    ResetSession,
    /// Load a past conversation so the next StartTask can follow up.
    ResumeSession(ConversationId),
    /// Verify the current provider settings without starting a task.
    TestConnection,
    /// Re-open the Knowledge Vault after Settings saves `vault_path`.
    ReloadKnowledge,
}

#[derive(Clone, Debug)]
pub struct StartTaskRequest {
    pub task_id: TaskId,
    pub prompt: String,
    pub context: ContextCapsule,
    pub conversation_id: ConversationId,
    pub mentions: Vec<Mention>,
}

impl StartTaskRequest {
    pub fn new(task_id: TaskId, prompt: impl Into<String>) -> Self {
        Self {
            task_id,
            prompt: prompt.into(),
            context: ContextCapsule::default(),
            conversation_id: ConversationId::new(),
            mentions: Vec::new(),
        }
    }
}
