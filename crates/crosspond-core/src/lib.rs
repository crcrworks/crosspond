//! Agent runtime types and the task loop.
//!
//! This crate must not depend on Tauri.

#![deny(unsafe_code)]

mod command;
mod config;
mod context;
mod conversation;
mod event;
mod history;
mod hotkey;
mod ids;
mod knowledge;
mod mention;
mod policy;
mod receipt;
mod runtime;
mod scratch;
mod secret;
mod status;

pub use command::{ApprovalId, RuntimeCommand, StartTaskRequest};
pub use config::{
    AppConfig, CHATGPT_SOURCE, ConfigError, ConfigStore, DEFAULT_CHATGPT_MODEL, DEFAULT_COMPAT_ID,
    DEFAULT_COMPAT_MODEL, FileConfigStore, OpenaiCompatEndpoint, ReasoningEffort, SelectedModel,
    default_vault_path, parse_vault_path_input, sanitize_compat_id,
};
pub use context::{
    AppContext, ContextCapsule, ContextCollector, MAX_AMBIENT_TEXT_CHARS, NullContextCollector,
    StagedInput, WindowContext,
};
pub use conversation::{
    ConversationView, TranscriptBlock, conversation_artifact_path, open_conversation,
};
pub use crosspond_knowledge::{
    FsVaultRepository, IndexedVault, KnowledgeId, KnowledgeNote, NoteKind, SearchHit, SearchIndex,
    VaultError, VaultRepository, index_db_path,
};
pub use crosspond_model::{
    ChatGptAuthorizationFlow, ChatGptOAuthTokens, ListedModel, REDIRECT_URI, TOKEN_URL,
    code_from_redirect, create_authorization_flow, ensure_model, exchange_authorization_code,
    fallback_chatgpt_models, fallback_compat_models, fetch_chatgpt_models, fetch_compat_models,
    parse_callback_input, refresh_chatgpt_session, wait_for_localhost_callback,
};
pub use event::AgentEvent;
pub use history::{TaskHistoryEntry, history_group_label, history_title, list_recent_tasks};
pub use hotkey::{GlobalHotkeyService, HotkeyEvent, HotkeySpecError, HotkeyView, LauncherHotkey};
pub use ids::{ConversationId, TaskId};
pub use mention::{Mention, display_prompt, parse_running_app_names};
pub use policy::{
    AgentAsk, BrowserHostDecision, ComputerApprovalMode, PolicyDecision, RiskLevel,
    browser_host_decision, evaluate, evaluate_with, risk_for_tool,
};
pub use receipt::{Receipt, receipt_action_line, tool_ui_summary};
pub use runtime::{
    CommandSender, EventPump, MISSING_API_KEY_MESSAGE, MISSING_CHATGPT_MESSAGE, RuntimeChannels,
    spawn_runtime, spawn_runtime_with, spawn_runtime_with_tools,
};
pub use scratch::default_tasks_root;
pub use secret::{
    SecretChatGptTokenStore, SecretError, SecretKey, SecretStore, SecretString,
    any_compat_key_is_set, chatgpt_oauth_is_set, compat_key_is_set, load_chatgpt_tokens,
    provider_is_ready, provider_key_is_set, save_chatgpt_tokens, selected_provider_is_ready,
};
pub use status::{CommandWindowState, TaskStatus};
