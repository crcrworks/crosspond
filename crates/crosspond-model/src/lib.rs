//! LLM provider abstraction. OpenAI-compatible streaming and tool calls.

#![deny(unsafe_code)]

mod error;
mod openai_compat;
mod provider;

pub use error::ModelError;
pub use openai_compat::{OpenAiCompatibleProvider, chat_completions_url, parse_sse_frames};
pub use provider::{
    EchoProvider, ImagePart, Message, ModelEvent, ModelProvider, ModelRequest, Role, ToolCall,
    ToolDefinition, default_provider_builder, keep_latest_images,
};

pub type ProviderBuilder =
    std::sync::Arc<dyn Fn(&str, &str, &str) -> std::sync::Arc<dyn ModelProvider> + Send + Sync>;
