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

/// Image part for vision models. `Debug` redacts bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct ImagePart {
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl std::fmt::Debug for ImagePart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImagePart")
            .field("media_type", &self.media_type)
            .field("bytes_len", &self.bytes.len())
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

#[derive(Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub images: Vec<ImagePart>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    pub encrypted_reasoning: Option<String>,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("role", &self.role)
            .field("content", &self.content)
            .field("images", &self.images)
            .field("tool_calls", &self.tool_calls)
            .field("tool_call_id", &self.tool_call_id)
            .field(
                "encrypted_reasoning",
                &self.encrypted_reasoning.as_ref().map(|_| "***"),
            )
            .finish()
    }
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            encrypted_reasoning: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            encrypted_reasoning: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            encrypted_reasoning: None,
        }
    }

    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: Vec::new(),
            tool_calls,
            tool_call_id: None,
            encrypted_reasoning: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            encrypted_reasoning: None,
        }
    }

    pub fn tool_with_images(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        images: Vec<ImagePart>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            images,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            encrypted_reasoning: None,
        }
    }

    pub fn with_encrypted_reasoning(mut self, encrypted: Option<String>) -> Self {
        self.encrypted_reasoning = encrypted.filter(|value| !value.is_empty());
        self
    }
}

/// Keep only the newest screenshot image in the conversation.
/// Older messages keep their text; images are cleared and a short note is appended.
pub fn keep_latest_images(messages: &mut [Message]) {
    let latest = messages
        .iter()
        .rposition(|message| !message.images.is_empty());
    let Some(latest) = latest else {
        return;
    };
    for (index, message) in messages.iter_mut().enumerate() {
        if index == latest || message.images.is_empty() {
            continue;
        }
        message.images.clear();
        if !message.content.contains("screenshot omitted") {
            if !message.content.is_empty() {
                message.content.push('\n');
            }
            message.content.push_str("[earlier screenshot omitted]");
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
    EncryptedReasoning(String),
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
    std::sync::Arc::new(|model, auth| match auth {
        crate::ProviderAuth::ApiKey { base_url, api_key } => {
            Arc::new(OpenAiCompatibleProvider::new(base_url, model, api_key))
        }
        crate::ProviderAuth::ChatGptOAuth { tokens, store } => {
            Arc::new(crate::ChatGptCodexProvider::new(model, tokens, store))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_latest_images_drops_older() {
        let mut messages = vec![
            Message::tool_with_images(
                "1",
                "first shot",
                vec![ImagePart {
                    media_type: "image/jpeg".into(),
                    bytes: vec![1],
                    width: Some(100),
                    height: Some(50),
                }],
            ),
            Message::assistant("ok"),
            Message::tool_with_images(
                "2",
                "second shot",
                vec![ImagePart {
                    media_type: "image/jpeg".into(),
                    bytes: vec![2, 2],
                    width: Some(200),
                    height: Some(100),
                }],
            ),
        ];
        keep_latest_images(&mut messages);
        assert!(messages[0].images.is_empty());
        assert!(messages[0].content.contains("screenshot omitted"));
        assert_eq!(messages[2].images.len(), 1);
        assert_eq!(messages[2].images[0].bytes, vec![2, 2]);
    }

    #[test]
    fn debug_redacts_encrypted_reasoning() {
        let message =
            Message::assistant("ok").with_encrypted_reasoning(Some("enc-secret-blob".into()));
        let rendered = format!("{message:?}");
        assert!(rendered.contains("***"));
        assert!(!rendered.contains("enc-secret-blob"));
    }
}
