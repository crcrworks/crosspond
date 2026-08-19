//! LLM provider abstraction. OpenAI-compatible streaming and ChatGPT Codex OAuth.

#![deny(unsafe_code)]

mod chatgpt_codex;
mod chatgpt_oauth;
mod error;
mod openai_compat;
mod provider;

pub use chatgpt_codex::{ChatGptCodexProvider, responses_body};
pub use chatgpt_oauth::{
    AUTHORIZE_URL, CLIENT_ID, CODEX_RESPONSES_URL, ChatGptAuthorizationFlow, ChatGptOAuthTokens,
    ChatGptPkce, ChatGptTokenStore, MemoryChatGptTokenStore, REDIRECT_URI, TOKEN_URL,
    create_authorization_flow, exchange_authorization_code, parse_callback_input,
    parse_token_response, refresh_access_token,
};
pub use error::ModelError;
pub use openai_compat::{OpenAiCompatibleProvider, chat_completions_url, parse_sse_frames};
pub use provider::{
    EchoProvider, ImagePart, Message, ModelEvent, ModelProvider, ModelRequest, Role, ToolCall,
    ToolDefinition, default_provider_builder, keep_latest_images,
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
