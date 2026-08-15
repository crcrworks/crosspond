use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::ModelError;
use crate::openai_compat::OpenAiCompatibleProvider;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Clone, Debug)]
pub enum ModelEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCall(ToolCall),
}

pub trait ModelProvider: Send + Sync {
    fn stream(
        &self,
        request: ModelRequest,
        events: UnboundedSender<ModelEvent>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>;

    fn test_connection(&self) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>>;
}

pub fn default_provider_builder() -> crate::ProviderBuilder {
    std::sync::Arc::new(|base_url, model, api_key| {
        Arc::new(OpenAiCompatibleProvider::new(base_url, model, api_key))
    })
}

pub struct EchoProvider {
    delay: std::time::Duration,
}

impl EchoProvider {
    pub fn new(delay: std::time::Duration) -> Self {
        Self { delay }
    }
}

impl ModelProvider for EchoProvider {
    fn stream(
        &self,
        request: ModelRequest,
        events: UnboundedSender<ModelEvent>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
        let delay = self.delay;
        let prompt = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == Role::User)
            .map(|message| message.content.clone())
            .unwrap_or_default();
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            let _ = events.send(ModelEvent::TextDelta(format!("You typed: {prompt}")));
            Ok(())
        })
    }

    fn test_connection(&self) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
}
