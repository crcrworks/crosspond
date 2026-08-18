//! Agent runtime types and the task loop.
//!
//! This crate must not depend on Tauri.

#![deny(unsafe_code)]

mod command;
mod config;
mod context;
mod event;
mod history;
mod hotkey;
mod ids;
mod knowledge;
mod policy;
mod receipt;
mod runtime;
mod scratch;
mod secret;
mod status;

pub use command::{ApprovalId, RuntimeCommand, StartTaskRequest};
pub use config::{AppConfig, ConfigError, ConfigStore, FileConfigStore, ProviderKind};
pub use context::{
    AppContext, ContextCapsule, ContextCollector, MAX_AMBIENT_TEXT_CHARS, NullContextCollector,
    StagedInput, WindowContext,
};
pub use crosspond_knowledge::{
    FsVaultRepository, IndexedVault, KnowledgeId, KnowledgeNote, NoteKind, SearchHit, SearchIndex,
    VaultError, VaultRepository, index_db_path,
};
pub use event::AgentEvent;
pub use history::{TaskHistoryEntry, history_group_label, history_title, list_recent_tasks};
pub use hotkey::{GlobalHotkeyService, HotkeyEvent};
pub use ids::TaskId;
pub use policy::{
    AgentAsk, ComputerApprovalMode, PolicyDecision, RiskLevel, evaluate, evaluate_with,
    risk_for_tool,
};
pub use receipt::{Receipt, receipt_action_line, tool_ui_summary};
pub use runtime::{
    CommandSender, EventPump, MAX_AGENT_STEPS, MISSING_API_KEY_MESSAGE, RuntimeChannels,
    spawn_runtime, spawn_runtime_with, spawn_runtime_with_tools,
};
pub use scratch::default_tasks_root;
pub use secret::{SecretError, SecretKey, SecretStore, SecretString, provider_key_is_set};
pub use status::{CommandWindowState, TaskStatus};
