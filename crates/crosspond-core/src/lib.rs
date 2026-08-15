//! Agent runtime types and the task loop.
//!
//! This crate must not depend on GPUI.

#![deny(unsafe_code)]

mod command;
mod config;
mod context;
mod event;
mod hotkey;
mod ids;
mod policy;
mod receipt;
mod runtime;
mod secret;
mod status;
mod workspace;

pub use command::{ApprovalId, RuntimeCommand, StartTaskRequest};
pub use config::{AppConfig, ConfigError, ConfigStore, FileConfigStore, ProviderKind};
pub use context::{
    AppContext, ContextCapsule, ContextCollector, MAX_AMBIENT_TEXT_CHARS, NullContextCollector,
    StagedInput, WindowContext,
};
pub use event::AgentEvent;
pub use hotkey::{GlobalHotkeyService, HotkeyEvent};
pub use ids::TaskId;
pub use policy::{
    AgentAsk, ComputerApprovalMode, PolicyDecision, RiskLevel, evaluate, evaluate_with,
    risk_for_tool,
};
pub use receipt::Receipt;
pub use runtime::{
    CommandSender, EventPump, MAX_AGENT_STEPS, MISSING_API_KEY_MESSAGE, RuntimeChannels,
    spawn_runtime, spawn_runtime_with, spawn_runtime_with_tools,
};
pub use secret::{SecretError, SecretKey, SecretStore, SecretString};
pub use status::{CommandWindowState, TaskStatus};
