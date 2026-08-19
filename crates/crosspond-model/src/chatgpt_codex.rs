use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;

use crate::chatgpt_oauth::{
    CODEX_CLIENT_VERSION, CODEX_RESPONSES_URL, ChatGptOAuthTokens, ChatGptTokenStore, ORIGINATOR,
    TOKEN_URL, chatgpt_refresh_lock, refresh_access_token,
};
use crate::error::ModelError;
use crate::list_models::fetch_chatgpt_models_at;
use crate::openai_compat::{ThinkSplitter, emit_split_piece, split_sse_frame};
use crate::provider::{
    ImagePart, Message, ModelEvent, ModelProvider, ModelRequest, Role, ToolCall, ToolDefinition,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct ChatGptCodexProvider {
    model: String,
    tokens: Arc<Mutex<ChatGptOAuthTokens>>,
    store: Arc<dyn ChatGptTokenStore>,
    http: Client,
    responses_url: String,
    token_url: String,
}

impl std::fmt::Debug for ChatGptCodexProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptCodexProvider")
            .field("model", &self.model)
            .field("tokens", &"***")
            .field("responses_url", &self.responses_url)
            .finish()
    }
}

impl ChatGptCodexProvider {
    pub fn new(
        model: impl Into<String>,
        tokens: ChatGptOAuthTokens,
        store: Arc<dyn ChatGptTokenStore>,
    ) -> Self {
        Self::with_endpoints(model, tokens, store, CODEX_RESPONSES_URL, TOKEN_URL)
    }

    pub fn with_endpoints(
        model: impl Into<String>,
        tokens: ChatGptOAuthTokens,
        store: Arc<dyn ChatGptTokenStore>,
        responses_url: impl Into<String>,
        token_url: impl Into<String>,
    ) -> Self {
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            model: model.into(),
            tokens: Arc::new(Mutex::new(tokens)),
            store,
            http,
            responses_url: responses_url.into(),
            token_url: token_url.into(),
        }
    }

    async fn ensure_fresh_tokens(&self) -> Result<ChatGptOAuthTokens, ModelError> {
        let _guard = chatgpt_refresh_lock().lock().await;
        let current = if let Ok(Some(stored)) = self.store.load() {
            *self.tokens.lock().unwrap_or_else(|err| err.into_inner()) = stored.clone();
            stored
        } else {
            self.tokens
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .clone()
        };
        if !current.needs_refresh() {
            return Ok(current);
        }
        self.refresh_tokens(&current.refresh).await
    }

    async fn recover_after_unauthorized(
        &self,
        previous: &ChatGptOAuthTokens,
    ) -> Result<ChatGptOAuthTokens, ModelError> {
        let _guard = chatgpt_refresh_lock().lock().await;
        if let Ok(Some(stored)) = self.store.load()
            && stored.access != previous.access
        {
            *self.tokens.lock().unwrap_or_else(|err| err.into_inner()) = stored.clone();
            return Ok(stored);
        }
        let refresh = self
            .store
            .load()
            .ok()
            .flatten()
            .map(|stored| stored.refresh)
            .unwrap_or_else(|| previous.refresh.clone());
        match self.refresh_tokens(&refresh).await {
            Ok(tokens) => Ok(tokens),
            Err(err) => {
                if let Ok(Some(stored)) = self.store.load()
                    && stored.access != previous.access
                {
                    *self.tokens.lock().unwrap_or_else(|err| err.into_inner()) = stored.clone();
                    return Ok(stored);
                }
                Err(err)
            }
        }
    }

    async fn refresh_tokens(&self, refresh: &str) -> Result<ChatGptOAuthTokens, ModelError> {
        let refreshed = refresh_access_token(refresh, &self.token_url).await?;
        self.store.save(&refreshed)?;
        *self.tokens.lock().unwrap_or_else(|err| err.into_inner()) = refreshed.clone();
        Ok(refreshed)
    }

    async fn stream_inner(
        self,
        request: ModelRequest,
        events: UnboundedSender<ModelEvent>,
        allow_empty: bool,
    ) -> Result<(), ModelError> {
        let mut tokens = self.ensure_fresh_tokens().await?;
        let body = responses_body(&self.model, &request);
        let mut response = self.post_responses(&tokens, &body).await?;
        if response.status().as_u16() == 401 {
            tokens = match self.recover_after_unauthorized(&tokens).await {
                Ok(tokens) => tokens,
                Err(_) => return Err(ModelError::Unauthorized),
            };
            response = self.post_responses(&tokens, &body).await?;
        }
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ModelError::from_status(status.as_u16(), &text));
        }

        let mut buffer = String::new();
        let mut got_text = false;
        let mut got_completed = false;
        let mut builders: HashMap<String, ToolCallBuilder> = HashMap::new();
        let mut encrypted_reasoning = None;
        let mut think = ThinkSplitter::default();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?
        {
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            let done = drain_responses_sse(&mut buffer, |data| {
                apply_responses_event(
                    data,
                    &events,
                    &mut got_text,
                    &mut builders,
                    &mut encrypted_reasoning,
                    &mut got_completed,
                    &mut think,
                )
            })?;
            if done {
                break;
            }
        }
        for piece in think.flush() {
            emit_split_piece(&events, piece, &mut got_text)?;
        }

        let mut calls: Vec<_> = builders.into_values().collect();
        calls.sort_by_key(|builder| builder.order);
        let mut got_tools = false;
        for builder in calls {
            if builder.name.is_empty() {
                continue;
            }
            got_tools = true;
            let id = if builder.call_id.is_empty() {
                builder.item_id
            } else {
                builder.call_id
            };
            events
                .send(ModelEvent::ToolCall(ToolCall {
                    id,
                    name: builder.name,
                    arguments: builder.arguments,
                }))
                .map_err(|_| ModelError::Network("event listener closed".into()))?;
        }
        if let Some(encrypted) = encrypted_reasoning.filter(|value| !value.is_empty()) {
            events
                .send(ModelEvent::EncryptedReasoning(encrypted))
                .map_err(|_| ModelError::Network("event listener closed".into()))?;
        }
        if !got_text && !got_tools && !allow_empty {
            return Err(ModelError::EmptyResponse);
        }
        let _ = got_completed;
        Ok(())
    }

    async fn post_responses(
        &self,
        tokens: &ChatGptOAuthTokens,
        body: &Value,
    ) -> Result<reqwest::Response, ModelError> {
        self.http
            .post(&self.responses_url)
            .bearer_auth(&tokens.access)
            .header("chatgpt-account-id", &tokens.account_id)
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", ORIGINATOR)
            .header("version", CODEX_CLIENT_VERSION)
            .header("accept", "text/event-stream")
            .json(body)
            .send()
            .await
            .map_err(|err| ModelError::Network(public_reqwest_error(&err)))
    }
}

impl ModelProvider for ChatGptCodexProvider {
    fn stream(
        &self,
        request: ModelRequest,
        events: UnboundedSender<ModelEvent>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
        Box::pin(self.clone().stream_inner(request, events, false))
    }

    fn test_connection(&self) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
        let provider = self.clone();
        Box::pin(async move {
            let tokens = provider.ensure_fresh_tokens().await?;
            fetch_chatgpt_models_at(&derived_models_url(&provider.responses_url), &tokens)
                .await
                .map(|_| ())
        })
    }
}

#[derive(Default)]
struct ToolCallBuilder {
    order: usize,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
}

fn apply_responses_event(
    data: &str,
    events: &UnboundedSender<ModelEvent>,
    got_text: &mut bool,
    builders: &mut HashMap<String, ToolCallBuilder>,
    encrypted_reasoning: &mut Option<String>,
    got_completed: &mut bool,
    think: &mut ThinkSplitter,
) -> Result<(), ModelError> {
    let value: Value = serde_json::from_str(data).unwrap_or(Value::Null);
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "response.output_text.delta" | "response.content_part.delta" => {
            if let Some(delta) = value
                .get("delta")
                .and_then(Value::as_str)
                .or_else(|| value.pointer("/delta/text").and_then(Value::as_str))
                && !delta.is_empty()
            {
                for piece in think.push(delta) {
                    emit_split_piece(events, piece, got_text)?;
                }
            }
        }
        "response.reasoning_summary_text.delta"
        | "response.reasoning_text.delta"
        | "response.reasoning.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str)
                && !delta.is_empty()
            {
                events
                    .send(ModelEvent::ReasoningDelta(delta.to_string()))
                    .map_err(|_| ModelError::Network("event listener closed".into()))?;
            }
        }
        "response.output_item.added" | "response.output_item.done" => {
            if let Some(item) = value.get("item") {
                ingest_output_item(item, builders, encrypted_reasoning);
            }
        }
        "response.function_call_arguments.delta" => {
            let key = item_key(&value);
            if !builders.contains_key(&key) {
                let order = builders.len();
                builders.insert(
                    key.clone(),
                    ToolCallBuilder {
                        order,
                        item_id: key.clone(),
                        ..ToolCallBuilder::default()
                    },
                );
            }
            if let Some(delta) = value.get("delta").and_then(Value::as_str)
                && let Some(builder) = builders.get_mut(&key)
            {
                builder.arguments.push_str(delta);
            }
        }
        "response.completed" => *got_completed = true,
        "response.failed" => {
            let snippet = value
                .pointer("/response/error/message")
                .or_else(|| value.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            return Err(ModelError::Provider(snippet.chars().take(200).collect()));
        }
        _ => {}
    }
    Ok(())
}

fn ingest_output_item(
    item: &Value,
    builders: &mut HashMap<String, ToolCallBuilder>,
    encrypted_reasoning: &mut Option<String>,
) {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    if item_type == "reasoning" {
        if let Some(encrypted) = item
            .get("encrypted_content")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            *encrypted_reasoning = Some(encrypted.to_string());
        }
        return;
    }
    if item_type != "function_call" {
        return;
    }
    let key = item
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| item.get("call_id").and_then(Value::as_str))
        .unwrap_or("call")
        .to_string();
    let order = builders.len();
    let builder = builders
        .entry(key.clone())
        .or_insert_with(|| ToolCallBuilder {
            order,
            item_id: key,
            ..ToolCallBuilder::default()
        });
    if let Some(call_id) = item.get("call_id").and_then(Value::as_str) {
        builder.call_id = call_id.to_string();
    }
    if let Some(name) = item.get("name").and_then(Value::as_str) {
        builder.name = name.to_string();
    }
    if let Some(arguments) = item.get("arguments").and_then(Value::as_str)
        && builder.arguments.is_empty()
    {
        builder.arguments = arguments.to_string();
    }
}

fn item_key(value: &Value) -> String {
    if let Some(id) = value.get("item_id").and_then(Value::as_str) {
        return id.to_string();
    }
    value
        .get("output_index")
        .map(|index| index.to_string())
        .unwrap_or_else(|| "call".into())
}

fn drain_responses_sse(
    buffer: &mut String,
    mut on_data: impl FnMut(&str) -> Result<(), ModelError>,
) -> Result<bool, ModelError> {
    let mut saw_completed = false;
    loop {
        let Some(frame) = split_sse_frame(buffer) else {
            return Ok(saw_completed);
        };
        for line in frame.lines() {
            let line = line.trim_end_matches('\r');
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                saw_completed = true;
            } else if !data.is_empty() {
                if data.contains("\"type\":\"response.completed\"")
                    || data.contains("\"response.completed\"")
                {
                    saw_completed = true;
                }
                on_data(data)?;
            }
        }
    }
}

pub fn responses_body(default_model: &str, request: &ModelRequest) -> Value {
    let model = if request.model.is_empty() {
        default_model
    } else {
        request.model.as_str()
    };
    let (instructions, input) = wire_input(&request.messages);
    let mut body = json!({
        "model": model,
        "input": input,
        "stream": true,
        "store": false,
        "include": ["reasoning.encrypted_content"],
    });
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions);
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(wire_tool).collect());
    }
    if let Some(effort) = request
        .reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["reasoning"] = json!({ "effort": effort });
    }
    body
}

fn wire_tool(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
    })
}

fn wire_input(messages: &[Message]) -> (String, Vec<Value>) {
    let mut instructions = String::new();
    let mut input = Vec::new();
    for message in messages {
        match message.role {
            Role::System => {
                if !instructions.is_empty() {
                    instructions.push_str("\n\n");
                }
                instructions.push_str(&message.content);
            }
            Role::User => input.push(message_item("user", &message.content, &message.images)),
            Role::Assistant => {
                if let Some(encrypted) = &message.encrypted_reasoning
                    && !encrypted.is_empty()
                {
                    input.push(json!({
                        "type": "reasoning",
                        "encrypted_content": encrypted,
                        "summary": []
                    }));
                }
                for call in &message.tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call.id,
                        "name": call.name,
                        "arguments": call.arguments,
                    }));
                }
                if !message.content.is_empty() {
                    input.push(message_item("assistant", &message.content, &[]));
                }
            }
            Role::Tool => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": message.tool_call_id.clone().unwrap_or_default(),
                    "output": message.content,
                }));
            }
        }
    }
    if let Some(source) = messages
        .iter()
        .rev()
        .find(|message| !message.images.is_empty())
        && source.role == Role::Tool
    {
        input.push(message_item(
            "user",
            &screenshot_vision_prompt(&source.images),
            &source.images,
        ));
    }
    (instructions, input)
}

fn message_item(role: &str, text: &str, images: &[ImagePart]) -> Value {
    let mut content = Vec::new();
    if !text.is_empty() {
        let kind = if role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        content.push(json!({ "type": kind, "text": text }));
    }
    if role != "assistant" {
        for image in images {
            let encoded =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image.bytes);
            content.push(json!({
                "type": "input_image",
                "image_url": format!("data:{};base64,{encoded}", image.media_type),
                "detail": "high",
            }));
        }
    }
    json!({
        "type": "message",
        "role": role,
        "content": content,
    })
}

fn screenshot_vision_prompt(images: &[ImagePart]) -> String {
    if let Some(image) = images.first()
        && let (Some(width), Some(height)) = (image.width, image.height)
    {
        return format!(
            "Screenshot image is {width}×{height} pixels (origin top-left). ui_click x,y must be integer pixels in this exact image; do not normalize each axis to 1000 or use macOS screen coordinates."
        );
    }
    "Screenshot image from the tool result above. ui_click uses exact pixels in this image (origin top-left); preserve its aspect ratio and do not normalize each axis to 1000.".into()
}

fn derived_models_url(responses_url: &str) -> String {
    if let Some(prefix) = responses_url.strip_suffix("/codex/responses") {
        return format!("{prefix}/codex/models");
    }
    if let Some(prefix) = responses_url.strip_suffix("/responses") {
        return format!("{prefix}/models");
    }
    crate::chatgpt_oauth::CODEX_MODELS_URL.to_string()
}

fn public_reqwest_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timed out".into()
    } else if err.is_connect() {
        "couldn’t connect".into()
    } else {
        "network error".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chatgpt_oauth::MemoryChatGptTokenStore;
    use crate::provider::ImagePart;
    use base64::Engine;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use tokio::sync::mpsc;

    fn sample_tokens() -> ChatGptOAuthTokens {
        ChatGptOAuthTokens {
            access: "access-token".into(),
            refresh: "refresh-token".into(),
            expires_at: crate::chatgpt_oauth::unix_ms() + 3_600_000,
            account_id: "acct_1".into(),
        }
    }

    fn provider_at(responses_url: &str, token_url: &str) -> ChatGptCodexProvider {
        let tokens = sample_tokens();
        ChatGptCodexProvider::with_endpoints(
            "gpt-5.2",
            tokens.clone(),
            MemoryChatGptTokenStore::new(tokens),
            responses_url,
            token_url,
        )
    }

    #[test]
    fn debug_redacts_tokens() {
        let provider = ChatGptCodexProvider::new(
            "gpt-5.2",
            sample_tokens(),
            MemoryChatGptTokenStore::new(sample_tokens()),
        );
        let rendered = format!("{provider:?}");
        assert!(rendered.contains("***"));
        assert!(!rendered.contains("access-token"));
    }

    #[test]
    fn maps_system_tools_and_encrypted_reasoning() {
        let body = responses_body(
            "gpt-5.2",
            &ModelRequest {
                model: String::new(),
                messages: vec![
                    Message::system("You are Crosspond."),
                    Message::user("hi"),
                    Message::assistant_tool_calls(
                        "checking",
                        vec![ToolCall {
                            id: "call_1".into(),
                            name: "list_apps".into(),
                            arguments: "{}".into(),
                        }],
                    )
                    .with_encrypted_reasoning(Some("enc-secret".into())),
                    Message::tool("call_1", "Safari"),
                ],
                tools: vec![ToolDefinition {
                    name: "list_apps".into(),
                    description: "List apps".into(),
                    parameters: json!({ "type": "object" }),
                }],
                reasoning_effort: None,
            },
        );
        assert_eq!(body["model"], "gpt-5.2");
        assert_eq!(body["store"], false);
        assert_eq!(body["instructions"], "You are Crosspond.");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["name"], "list_apps");
        assert!(body.get("max_output_tokens").is_none());
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[1]["type"], "reasoning");
        assert_eq!(input[1]["encrypted_content"], "enc-secret");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "call_1");
        assert_eq!(input[3]["type"], "message");
        assert_eq!(input[4]["type"], "function_call_output");
        assert_eq!(input[4]["output"], "Safari");
    }

    #[test]
    fn responses_body_includes_reasoning_effort() {
        let body = responses_body(
            "gpt-5.6-luna",
            &ModelRequest {
                model: String::new(),
                messages: vec![Message::user("hi")],
                tools: Vec::new(),
                reasoning_effort: Some("high".into()),
            },
        );
        assert_eq!(body["model"], "gpt-5.6-luna");
        assert_eq!(body["reasoning"]["effort"], "high");
    }

    #[test]
    fn responses_body_omits_reasoning_object_without_effort() {
        let body = responses_body(
            "gpt-5.6-luna",
            &ModelRequest {
                model: String::new(),
                messages: vec![Message::user("hi")],
                tools: Vec::new(),
                reasoning_effort: None,
            },
        );
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn tool_images_become_follow_up_user_message() {
        let body = responses_body(
            "gpt-5.2",
            &ModelRequest {
                model: String::new(),
                messages: vec![
                    Message::user("click continue"),
                    Message::assistant_tool_calls(
                        String::new(),
                        vec![ToolCall {
                            id: "call_1".into(),
                            name: "take_screenshot".into(),
                            arguments: "{}".into(),
                        }],
                    ),
                    Message::tool_with_images(
                        "call_1",
                        "Screenshot of Safari (100×50).",
                        vec![ImagePart {
                            media_type: "image/jpeg".into(),
                            bytes: vec![1, 2, 3],
                            width: Some(100),
                            height: Some(50),
                        }],
                    ),
                ],
                tools: Vec::new(),
                reasoning_effort: None,
            },
        );
        let input = body["input"].as_array().unwrap();
        let tool = input
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .unwrap();
        assert_eq!(tool["output"], "Screenshot of Safari (100×50).");
        let vision = input.last().unwrap();
        assert_eq!(vision["role"], "user");
        let parts = vision["content"].as_array().unwrap();
        assert_eq!(parts[0]["type"], "input_text");
        assert!(parts[0]["text"].as_str().unwrap().contains("100×50 pixels"));
        assert_eq!(parts[1]["type"], "input_image");
        assert_eq!(parts[1]["detail"], "high");
        assert!(
            parts[1]["image_url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
    }

    #[tokio::test]
    async fn streams_text_and_reasoning() {
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\"}\n\n\
             data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"plan\"}\n\n\
             data: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\n\
             data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_http(200, "text/event-stream", sse.as_bytes());
        let provider = provider_at(&format!("{base}/codex/responses"), &format!("{base}/token"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap();
        let mut text = String::new();
        let mut reasoning = String::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                ModelEvent::TextDelta(piece) => text.push_str(&piece),
                ModelEvent::ReasoningDelta(piece) => reasoning.push_str(&piece),
                _ => {}
            }
        }
        assert_eq!(text, "Hello");
        assert_eq!(reasoning, "plan");
    }

    #[tokio::test]
    async fn splits_think_tags_in_output_text() {
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"<think>plan</think>Hi\"}\n\n\
             data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_http(200, "text/event-stream", sse.as_bytes());
        let provider = provider_at(&format!("{base}/codex/responses"), &format!("{base}/token"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap();
        let mut text = String::new();
        let mut reasoning = String::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                ModelEvent::TextDelta(piece) => text.push_str(&piece),
                ModelEvent::ReasoningDelta(piece) => reasoning.push_str(&piece),
                _ => {}
            }
        }
        assert_eq!(text, "Hi");
        assert_eq!(reasoning, "plan");
    }

    #[tokio::test]
    async fn streams_tool_calls_and_encrypted_reasoning() {
        let sse = "data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"write_file\",\"arguments\":\"\"}}\n\n\
             data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_1\",\"delta\":\"{\\\"path\\\":\\\"output/a.txt\\\"}\"}\n\n\
             data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"reasoning\",\"encrypted_content\":\"enc-1\"}}\n\n\
             data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_http(200, "text/event-stream", sse.as_bytes());
        let provider = provider_at(&format!("{base}/codex/responses"), &format!("{base}/token"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("write")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap();
        let mut tool = None;
        let mut encrypted = None;
        while let Ok(event) = rx.try_recv() {
            match event {
                ModelEvent::ToolCall(call) => tool = Some(call),
                ModelEvent::EncryptedReasoning(value) => encrypted = Some(value),
                _ => {}
            }
        }
        let tool = tool.expect("tool");
        assert_eq!(tool.id, "call_1");
        assert_eq!(tool.name, "write_file");
        assert_eq!(tool.arguments, r#"{"path":"output/a.txt"}"#);
        assert_eq!(encrypted.as_deref(), Some("enc-1"));
    }

    #[tokio::test]
    async fn empty_completed_is_not_success() {
        let sse = "data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_http(200, "text/event-stream", sse.as_bytes());
        let provider = provider_at(&format!("{base}/codex/responses"), &format!("{base}/token"));
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ModelError::EmptyResponse));
    }

    #[tokio::test]
    async fn maps_401() {
        let base = serve_http(401, "application/json", br#"{"error":{"message":"nope"}}"#);
        let provider = provider_at(
            &format!("{base}/codex/responses"),
            &format!("{base}/dead-token"),
        );
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ModelError::Unauthorized));
        assert!(!err.user_message().contains("nope"));
    }

    #[tokio::test]
    async fn retries_after_401_refresh() {
        let access = jwt_with_account("acct_refreshed");
        let refresh_body = serde_json::json!({
            "access_token": access,
            "refresh_token": "refresh-2",
            "expires_in": 3600
        })
        .to_string();
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
             data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_script(vec![
            (
                401,
                "application/json",
                br#"{"error":{"message":"expired"}}"#.to_vec(),
            ),
            (200, "application/json", refresh_body.into_bytes()),
            (200, "text/event-stream", sse.as_bytes().to_vec()),
        ]);
        let tokens = sample_tokens();
        let store = MemoryChatGptTokenStore::new(tokens.clone());
        let provider = ChatGptCodexProvider::with_endpoints(
            "gpt-5.2",
            tokens,
            store.clone(),
            format!("{base}/codex/responses"),
            format!("{base}/token"),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap();
        let mut text = String::new();
        while let Ok(event) = rx.try_recv() {
            if let ModelEvent::TextDelta(piece) = event {
                text.push_str(&piece);
            }
        }
        assert_eq!(text, "ok");
        assert_eq!(store.current().unwrap().refresh, "refresh-2");
        assert_eq!(store.current().unwrap().account_id, "acct_refreshed");
    }

    #[tokio::test]
    async fn refreshes_expired_token_before_stream() {
        let access = jwt_with_account("acct_pre");
        let refresh_body = serde_json::json!({
            "access_token": access,
            "refresh_token": "refresh-pre",
            "expires_in": 3600
        })
        .to_string();
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ready\"}\n\n\
             data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_script(vec![
            (200, "application/json", refresh_body.into_bytes()),
            (200, "text/event-stream", sse.as_bytes().to_vec()),
        ]);
        let mut tokens = sample_tokens();
        tokens.expires_at = crate::chatgpt_oauth::unix_ms() - 1;
        let store = MemoryChatGptTokenStore::new(tokens.clone());
        let provider = ChatGptCodexProvider::with_endpoints(
            "gpt-5.2",
            tokens,
            store.clone(),
            format!("{base}/codex/responses"),
            format!("{base}/token"),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap();
        let mut text = String::new();
        while let Ok(event) = rx.try_recv() {
            if let ModelEvent::TextDelta(piece) = event {
                text.push_str(&piece);
            }
        }
        assert_eq!(text, "ready");
        assert_eq!(store.current().unwrap().refresh, "refresh-pre");
    }

    #[tokio::test]
    async fn uses_tokens_already_saved_to_the_store() {
        let mut stale = sample_tokens();
        stale.expires_at = crate::chatgpt_oauth::unix_ms() - 1;
        let mut stored = sample_tokens();
        stored.access = "stored-access".into();
        stored.expires_at = crate::chatgpt_oauth::unix_ms() + 3_600_000;
        let store = MemoryChatGptTokenStore::new(stored);
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ready\"}\n\n\
             data: {\"type\":\"response.completed\"}\n\n";
        let base = serve_script(vec![(200, "text/event-stream", sse.as_bytes().to_vec())]);
        let provider = ChatGptCodexProvider::with_endpoints(
            "gpt-5.2",
            stale,
            store,
            format!("{base}/codex/responses"),
            format!("{base}/missing-token"),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                    reasoning_effort: None,
                },
                tx,
            )
            .await
            .unwrap();
        let mut text = String::new();
        while let Ok(event) = rx.try_recv() {
            if let ModelEvent::TextDelta(piece) = event {
                text.push_str(&piece);
            }
        }
        assert_eq!(text, "ready");
    }

    #[tokio::test]
    async fn maps_usage_limit() {
        let base = serve_http(
            404,
            "application/json",
            br#"{"error":{"code":"usage_limit_reached"}}"#,
        );
        let provider = provider_at(&format!("{base}/codex/responses"), &format!("{base}/token"));
        let err = provider.test_connection().await.unwrap_err();
        assert!(matches!(err, ModelError::UsageLimited));
        assert!(err.user_message().contains("usage limit"));
    }

    #[tokio::test]
    async fn test_connection_accepts_models_list() {
        let base = serve_http(
            200,
            "application/json",
            br#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
        );
        let provider = provider_at(&format!("{base}/codex/responses"), &format!("{base}/token"));
        provider.test_connection().await.unwrap();
    }

    fn jwt_with_account(account: &str) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = serde_json::json!({
            crate::chatgpt_oauth::JWT_AUTH_CLAIM: { "chatgpt_account_id": account }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{payload}.sig")
    }

    fn serve_http(status: u16, content_type: &str, body: &[u8]) -> String {
        serve_script(vec![(status, content_type, body.to_vec())])
    }

    fn serve_script(replies: Vec<(u16, &str, Vec<u8>)>) -> String {
        let replies: Vec<(u16, String, Vec<u8>)> = replies
            .into_iter()
            .map(|(status, content_type, body)| (status, content_type.to_string(), body))
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for (status, content_type, body) in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let header = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        format!("http://{addr}")
    }
}
