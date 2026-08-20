//! LLM provider abstraction. OpenAI-compatible streaming and ChatGPT Codex OAuth.

#![deny(unsafe_code)]

mod chatgpt_codex;
mod chatgpt_oauth;
mod error;
mod list_models;
mod openai_compat;
mod provider;

pub use chatgpt_codex::{ChatGptCodexProvider, responses_body};
pub use chatgpt_oauth::{
    AUTHORIZE_URL, CLIENT_ID, CODEX_CLIENT_VERSION, CODEX_MODELS_URL, CODEX_RESPONSES_URL,
    ChatGptAuthorizationFlow, ChatGptOAuthTokens, ChatGptPkce, ChatGptTokenStore,
    LocalhostCallback, MemoryChatGptTokenStore, ORIGINATOR, REDIRECT_URI, TOKEN_URL,
    classify_callback_url, classify_localhost_http_request, code_from_redirect,
    create_authorization_flow, exchange_authorization_code, parse_callback_input,
    parse_token_response, refresh_access_token, refresh_chatgpt_session,
    wait_for_localhost_callback,
};
pub use error::ModelError;
pub use list_models::{
    ListedModel, ensure_model, fallback_chatgpt_models, fallback_compat_models,
    fetch_chatgpt_models, fetch_chatgpt_models_at, fetch_compat_models, fetch_compat_models_at,
    parse_models_json,
};
pub use openai_compat::{
    OpenAiCompatibleProvider, chat_completions_url, models_url, parse_sse_frames,
};
pub use provider::{
    EchoProvider, ImagePart, ImageSource, Message, ModelEvent, ModelProvider, ModelRequest, Role,
    ToolCall, ToolDefinition, default_provider_builder, keep_latest_images,
};

pub enum ProviderAuth {
    ApiKey {
        base_url: String,
        api_key: String,
    },
    ChatGptOAuth {
        tokens: ChatGptOAuthTokens,
        store: std::sync::Arc<dyn ChatGptTokenStore>,
    },
}

pub type ProviderBuilder =
    std::sync::Arc<dyn Fn(&str, ProviderAuth) -> std::sync::Arc<dyn ModelProvider> + Send + Sync>;
