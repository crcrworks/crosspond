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
}

impl std::fmt::Debug for ImagePart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImagePart")
            .field("media_type", &self.media_type)
            .field("bytes_len", &self.bytes.len())
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub images: Vec<ImagePart>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            images: Vec::new(),
            tool_calls,
            tool_call_id: None,
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
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
        }
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
                }],
            ),
            Message::assistant("ok"),
            Message::tool_with_images(
                "2",
                "second shot",
                vec![ImagePart {
                    media_type: "image/jpeg".into(),
                    bytes: vec![2, 2],
                }],
            ),
        ];
        keep_latest_images(&mut messages);
        assert!(messages[0].images.is_empty());
        assert!(messages[0].content.contains("screenshot omitted"));
        assert_eq!(messages[2].images.len(), 1);
        assert_eq!(messages[2].images[0].bytes, vec![2, 2]);
    }
}
