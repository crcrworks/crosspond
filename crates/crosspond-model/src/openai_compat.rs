use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::ModelError;
use crate::provider::{Message, ModelEvent, ModelProvider, ModelRequest, Role, ToolCall};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct OpenAiCompatibleProvider {
    base_url: String,
    model: String,
    api_key: String,
    http: Client,
}

impl std::fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompatibleProvider")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"***")
            .finish()
    }
}

impl OpenAiCompatibleProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            base_url: base_url.into(),
            model: model.into(),
            api_key: api_key.into(),
            http,
        }
    }

    async fn stream_inner(
        self,
        request: ModelRequest,
        events: UnboundedSender<ModelEvent>,
    ) -> Result<(), ModelError> {
        let body = ChatRequestBody {
            model: if request.model.is_empty() {
                self.model.clone()
            } else {
                request.model
            },
            messages: request.messages.iter().map(WireMessage::from).collect(),
            stream: true,
            max_tokens: None,
            tools: request.tools.iter().map(WireTool::from).collect(),
        };

        let response = self
            .http
            .post(chat_completions_url(&self.base_url))
            .bearer_auth(&self.api_key)
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError::from_status(status.as_u16(), &body));
        }

        let mut buffer = String::new();
        let mut got_text = false;
        let mut builders: HashMap<usize, ToolCallBuilder> = HashMap::new();
        let mut think = ThinkSplitter::default();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?
        {
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            let done = drain_sse(&mut buffer, |data| {
                if let Some(delta) = reasoning_delta_from_chunk(data)?
                    && events.send(ModelEvent::ReasoningDelta(delta)).is_err()
                {
                    return Err(ModelError::Network("event listener closed".into()));
                }
                if let Some(delta) = content_delta_from_chunk(data)? {
                    for piece in think.push(&delta) {
                        emit_split_piece(&events, piece, &mut got_text)?;
                    }
                }
                for (index, delta) in tool_call_deltas_from_chunk(data)? {
                    let builder = builders.entry(index).or_default();
                    if let Some(id) = delta.id {
                        builder.id = id;
                    }
                    if let Some(name) = delta.name {
                        builder.name = name;
                    }
                    if let Some(arguments) = delta.arguments {
                        builder.arguments.push_str(&arguments);
                    }
                }
                Ok(())
            })?;
            if done {
                break;
            }
        }
        for piece in think.flush() {
            emit_split_piece(&events, piece, &mut got_text)?;
        }
        let mut calls: Vec<_> = builders.into_iter().collect();
        calls.sort_by_key(|(index, _)| *index);
        let mut got_tools = false;
        for (index, builder) in calls {
            if builder.name.is_empty() {
                continue;
            }
            got_tools = true;
            let id = if builder.id.is_empty() {
                format!("call_{index}")
            } else {
                builder.id
            };
            if events
                .send(ModelEvent::ToolCall(ToolCall {
                    id,
                    name: builder.name,
                    arguments: builder.arguments,
                }))
                .is_err()
            {
                return Err(ModelError::Network("event listener closed".into()));
            }
        }
        if !got_text && !got_tools {
            return Err(ModelError::EmptyResponse);
        }
        Ok(())
    }

    async fn test_inner(self) -> Result<(), ModelError> {
        let body = ChatRequestBody {
            model: self.model.clone(),
            messages: vec![WireMessage {
                role: "user",
                content: "ping".into(),
                tool_call_id: None,
                tool_calls: Vec::new(),
            }],
            stream: false,
            max_tokens: Some(1),
            tools: Vec::new(),
        };
        let response = self
            .http
            .post(chat_completions_url(&self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|err| ModelError::Network(public_reqwest_error(&err)))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ModelError::from_status(status.as_u16(), &body));
        }
        let value: serde_json::Value =
            serde_json::from_str(&body).map_err(|_| ModelError::EmptyResponse)?;
        if value.pointer("/choices/0").is_none() {
            return Err(ModelError::EmptyResponse);
        }
        Ok(())
    }
}

impl ModelProvider for OpenAiCompatibleProvider {
    fn stream(
        &self,
        request: ModelRequest,
        events: UnboundedSender<ModelEvent>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
        Box::pin(self.clone().stream_inner(request, events))
    }

    fn test_connection(&self) -> Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send>> {
        Box::pin(self.clone().test_inner())
    }
}

#[derive(Serialize)]
struct ChatRequestBody {
    model: String,
    messages: Vec<WireMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool>,
}

#[derive(Serialize)]
struct WireTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunction,
}

impl From<&crate::provider::ToolDefinition> for WireTool {
    fn from(tool: &crate::provider::ToolDefinition) -> Self {
        Self {
            kind: "function",
            function: WireFunction {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            },
        }
    }
}

#[derive(Serialize)]
struct WireFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct WireMessage {
    role: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireToolCall>,
}

#[derive(Serialize)]
struct WireToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireToolCallFn,
}

#[derive(Serialize)]
struct WireToolCallFn {
    name: String,
    arguments: String,
}

impl From<&Message> for WireMessage {
    fn from(message: &Message) -> Self {
        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        Self {
            role,
            content: message.content.clone(),
            tool_call_id: message.tool_call_id.clone(),
            tool_calls: message
                .tool_calls
                .iter()
                .map(|call| WireToolCall {
                    id: call.id.clone(),
                    kind: "function",
                    function: WireToolCallFn {
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    },
                })
                .collect(),
        }
    }
}

#[derive(Default)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

struct PartialToolDelta {
    id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
}

pub fn chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if has_openai_v1_prefix(base) {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

fn has_openai_v1_prefix(base: &str) -> bool {
    base.ends_with("/v1") || base.contains("/v1/")
}

/// Parse complete SSE frames from `buffer`. Returns true when `[DONE]` was seen.
pub fn parse_sse_frames(
    buffer: &mut String,
    mut on_data: impl FnMut(&str) -> Result<(), ModelError>,
) -> Result<bool, ModelError> {
    drain_sse(buffer, &mut on_data)
}

fn drain_sse(
    buffer: &mut String,
    mut on_data: impl FnMut(&str) -> Result<(), ModelError>,
) -> Result<bool, ModelError> {
    loop {
        let Some(frame) = split_sse_frame(buffer) else {
            return Ok(false);
        };
        let mut done = false;
        for line in frame.lines() {
            let line = line.trim_end_matches('\r');
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                done = true;
            } else if !data.is_empty() {
                on_data(data)?;
            }
        }
        if done {
            return Ok(true);
        }
    }
}

fn split_sse_frame(buffer: &mut String) -> Option<String> {
    let (idx, sep_len) = buffer
        .find("\r\n\r\n")
        .map(|i| (i, 4))
        .or_else(|| buffer.find("\n\n").map(|i| (i, 2)))?;
    let frame = buffer[..idx].to_string();
    *buffer = buffer[idx + sep_len..].to_string();
    Some(frame)
}

fn emit_split_piece(
    events: &UnboundedSender<ModelEvent>,
    piece: SplitPiece,
    got_text: &mut bool,
) -> Result<(), ModelError> {
    match piece {
        SplitPiece::Text(text) => {
            *got_text = true;
            events
                .send(ModelEvent::TextDelta(text))
                .map_err(|_| ModelError::Network("event listener closed".into()))
        }
        SplitPiece::Think(text) => events
            .send(ModelEvent::ReasoningDelta(text))
            .map_err(|_| ModelError::Network("event listener closed".into())),
    }
}

#[derive(Default)]
struct ThinkSplitter {
    in_think: bool,
    pending: String,
}

enum SplitPiece {
    Text(String),
    Think(String),
}

impl ThinkSplitter {
    fn push(&mut self, chunk: &str) -> Vec<SplitPiece> {
        if chunk.is_empty() {
            return Vec::new();
        }
        self.pending.push_str(chunk);
        let mut out = Vec::new();
        loop {
            if self.in_think {
                if let Some(idx) = self.pending.find("</think>") {
                    let body = self.pending[..idx].to_string();
                    if !body.is_empty() {
                        out.push(SplitPiece::Think(body));
                    }
                    self.pending.replace_range(..idx + "</think>".len(), "");
                    self.in_think = false;
                } else {
                    drain_incomplete(&mut self.pending, "</think>", SplitPiece::Think, &mut out);
                    break;
                }
            } else if let Some(idx) = self.pending.find("<think>") {
                let body = self.pending[..idx].to_string();
                if !body.is_empty() {
                    out.push(SplitPiece::Text(body));
                }
                self.pending.replace_range(..idx + "<think>".len(), "");
                self.in_think = true;
            } else {
                drain_incomplete(&mut self.pending, "<think>", SplitPiece::Text, &mut out);
                break;
            }
        }
        out
    }

    fn flush(&mut self) -> Vec<SplitPiece> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.pending);
        if self.in_think {
            vec![SplitPiece::Think(rest)]
        } else {
            vec![SplitPiece::Text(rest)]
        }
    }
}

fn drain_incomplete(
    pending: &mut String,
    tag: &str,
    wrap: fn(String) -> SplitPiece,
    out: &mut Vec<SplitPiece>,
) {
    let keep = partial_tag_suffix(pending, tag);
    let cut = pending.len().saturating_sub(keep);
    if cut == 0 {
        return;
    }
    let body = pending[..cut].to_string();
    pending.replace_range(..cut, "");
    if !body.is_empty() {
        out.push(wrap(body));
    }
}

fn partial_tag_suffix(pending: &str, tag: &str) -> usize {
    let max = tag.len().saturating_sub(1).min(pending.len());
    for n in (1..=max).rev() {
        let start = pending.len() - n;
        if pending.is_char_boundary(start) && tag.starts_with(&pending[start..]) {
            return n;
        }
    }
    0
}

fn reasoning_delta_from_chunk(data: &str) -> Result<Option<String>, ModelError> {
    let value: serde_json::Value =
        serde_json::from_str(data).map_err(|err| ModelError::Provider(err.to_string()))?;
    let Some(delta) = value.pointer("/choices/0/delta") else {
        return Ok(None);
    };
    for key in [
        "reasoning_content",
        "reasoning",
        "thinking",
        "reasoning_text",
    ] {
        if let Some(text) = delta
            .get(key)
            .and_then(|value| value.as_str())
            .filter(|text| !text.is_empty())
        {
            return Ok(Some(text.to_string()));
        }
    }
    Ok(None)
}

fn content_delta_from_chunk(data: &str) -> Result<Option<String>, ModelError> {
    let value: serde_json::Value =
        serde_json::from_str(data).map_err(|err| ModelError::Provider(err.to_string()))?;
    let content = value
        .pointer("/choices/0/delta/content")
        .and_then(|value| value.as_str())
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    Ok(content)
}

fn tool_call_deltas_from_chunk(data: &str) -> Result<Vec<(usize, PartialToolDelta)>, ModelError> {
    let value: serde_json::Value =
        serde_json::from_str(data).map_err(|err| ModelError::Provider(err.to_string()))?;
    let Some(array) = value
        .pointer("/choices/0/delta/tool_calls")
        .and_then(|value| value.as_array())
    else {
        return Ok(Vec::new());
    };
    let mut deltas = Vec::new();
    for item in array {
        let index = item
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        let id = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty());
        let name = item
            .pointer("/function/name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty());
        let arguments = item
            .pointer("/function/arguments")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        deltas.push((
            index,
            PartialToolDelta {
                id,
                name,
                arguments,
            },
        ));
    }
    Ok(deltas)
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;
    use crate::provider::Message;

    #[test]
    fn builds_chat_completions_url() {
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://example.com/v1/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("http://127.0.0.1:1234"),
            "http://127.0.0.1:1234/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("http://127.0.0.1:1234/"),
            "http://127.0.0.1:1234/v1/chat/completions"
        );
    }

    #[test]
    fn debug_redacts_api_key() {
        let provider =
            OpenAiCompatibleProvider::new("https://api.openai.com/v1", "gpt", "sk-secret");
        let rendered = format!("{provider:?}");
        assert!(rendered.contains("***"));
        assert!(!rendered.contains("sk-secret"));
    }

    #[test]
    fn parses_sse_text_deltas() {
        let mut buffer = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
             data: [DONE]\n\n"
            .to_string();
        let mut pieces = Vec::new();
        let done = parse_sse_frames(&mut buffer, |data| {
            if let Some(delta) = content_delta_from_chunk(data)? {
                pieces.push(delta);
            }
            Ok(())
        })
        .unwrap();
        assert!(done);
        assert_eq!(pieces, ["Hel", "lo"]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn parses_reasoning_content_delta() {
        let data = r#"{"choices":[{"delta":{"reasoning_content":"plan"}}]}"#;
        assert_eq!(
            reasoning_delta_from_chunk(data).unwrap().as_deref(),
            Some("plan")
        );
        let data = r#"{"choices":[{"delta":{"content":"hi"}}]}"#;
        assert!(reasoning_delta_from_chunk(data).unwrap().is_none());
    }

    #[test]
    fn splits_think_tags_across_chunks() {
        let mut splitter = ThinkSplitter::default();
        let mut text = String::new();
        let mut think = String::new();
        for chunk in ["Hello <th", "ink>secret", " plan</th", "ink> world"] {
            for piece in splitter.push(chunk) {
                match piece {
                    SplitPiece::Text(piece) => text.push_str(&piece),
                    SplitPiece::Think(piece) => think.push_str(&piece),
                }
            }
        }
        for piece in splitter.flush() {
            match piece {
                SplitPiece::Text(piece) => text.push_str(&piece),
                SplitPiece::Think(piece) => think.push_str(&piece),
            }
        }
        assert_eq!(text, "Hello  world");
        assert_eq!(think, "secret plan");
    }

    #[test]
    fn short_text_is_emitted_before_stream_end() {
        let mut splitter = ThinkSplitter::default();
        let pieces = splitter.push("Hi");
        assert!(matches!(&pieces[..], [SplitPiece::Text(text)] if text == "Hi"));
    }

    #[test]
    fn leaves_incomplete_frame_in_buffer() {
        let mut buffer = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}"
            .to_string();
        let mut pieces = Vec::new();
        let done = parse_sse_frames(&mut buffer, |data| {
            if let Some(delta) = content_delta_from_chunk(data)? {
                pieces.push(delta);
            }
            Ok(())
        })
        .unwrap();
        assert!(!done);
        assert_eq!(pieces, ["Hel"]);
        assert!(buffer.contains("lo"));
    }

    #[tokio::test]
    async fn streams_text_deltas_from_mock_http() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
                    data: [DONE]\n\n";
        let base = serve_http(200, "text/event-stream", body.as_bytes(), None);
        let provider = OpenAiCompatibleProvider::new(base, "gpt-4o-mini", "sk-test");
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                },
                tx,
            )
            .await
            .unwrap();
        let mut pieces = Vec::new();
        while let Ok(ModelEvent::TextDelta(text)) = rx.try_recv() {
            pieces.push(text);
        }
        assert_eq!(pieces.concat(), "Hello");
    }

    #[tokio::test]
    async fn streams_tool_calls_from_mock_http() {
        let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"write_file\",\"arguments\":\"\"}}]}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"output/a.txt\\\"}\"}}]}}]}\n\n\
                    data: [DONE]\n\n";
        let base = serve_http(200, "text/event-stream", body.as_bytes(), None);
        let provider = OpenAiCompatibleProvider::new(base, "gpt-4o-mini", "sk-test");
        let (tx, mut rx) = mpsc::unbounded_channel();
        provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("write a file")],
                    tools: Vec::new(),
                },
                tx,
            )
            .await
            .unwrap();
        let event = rx.try_recv().expect("tool call");
        match event {
            ModelEvent::ToolCall(call) => {
                assert_eq!(call.id, "call_1");
                assert_eq!(call.name, "write_file");
                assert_eq!(call.arguments, r#"{"path":"output/a.txt"}"#);
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn empty_200_is_not_success() {
        let base = serve_http(200, "text/plain", b"", None);
        let provider = OpenAiCompatibleProvider::new(base, "gpt-4o-mini", "sk-test");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                },
                tx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ModelError::EmptyResponse));
    }

    #[tokio::test]
    async fn maps_401() {
        let base = serve_http(
            401,
            "application/json",
            br#"{"error":{"message":"Incorrect API key"}}"#,
            None,
        );
        let provider = OpenAiCompatibleProvider::new(base, "gpt-4o-mini", "sk-bad");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = provider
            .stream(
                ModelRequest {
                    model: String::new(),
                    messages: vec![Message::user("hi")],
                    tools: Vec::new(),
                },
                tx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ModelError::Unauthorized));
        assert!(err.user_message().contains("401"));
        assert!(!err.user_message().contains("Incorrect API key"));
    }

    #[tokio::test]
    async fn maps_429() {
        let base = serve_http(
            429,
            "application/json",
            br#"{"error":{"message":"slow down"}}"#,
            None,
        );
        let provider = OpenAiCompatibleProvider::new(base, "gpt-4o-mini", "sk-test");
        let err = provider.test_connection().await.unwrap_err();
        assert!(matches!(err, ModelError::RateLimited));
    }

    #[tokio::test]
    async fn cancel_aborts_in_flight_stream() {
        let base = serve_slow_sse();
        let provider = OpenAiCompatibleProvider::new(base, "gpt-4o-mini", "sk-test");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let fut = provider.stream(
            ModelRequest {
                model: String::new(),
                messages: vec![Message::user("hi")],
                tools: Vec::new(),
            },
            tx,
        );
        let handle = tokio::spawn(fut);
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("first delta")
            .expect("delta");
        assert!(matches!(first, ModelEvent::TextDelta(text) if text == "Hi"));
        handle.abort();
        let joined = handle.await;
        assert!(joined.unwrap_err().is_cancelled());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
    }

    fn serve_http(
        status: u16,
        content_type: &str,
        body: &[u8],
        delay_before_write: Option<Duration>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_vec();
        let content_type = content_type.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream);
            if let Some(delay) = delay_before_write {
                thread::sleep(delay);
            }
            let header = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        format!("http://{addr}/v1")
    }

    fn serve_slow_sse() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            write_chunk(
                &mut stream,
                b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            );
            thread::sleep(Duration::from_secs(30));
            write_chunk(&mut stream, b"data: [DONE]\n\n");
            stream.write_all(b"0\r\n\r\n").ok();
        });
        format!("http://{addr}/v1")
    }

    fn write_chunk(stream: &mut std::net::TcpStream, data: &[u8]) {
        write!(stream, "{:x}\r\n", data.len()).unwrap();
        stream.write_all(data).unwrap();
        stream.write_all(b"\r\n").unwrap();
        stream.flush().unwrap();
    }

    fn read_http_request(stream: &mut std::net::TcpStream) {
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(header_end) = find_double_newline(&buf) {
                        let content_len = content_length(&buf[..header_end]).unwrap_or(0);
                        if buf.len() >= header_end + content_len {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn find_double_newline(buf: &[u8]) -> Option<usize> {
        buf.windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
            .or_else(|| buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2))
    }

    fn content_length(headers: &[u8]) -> Option<usize> {
        let text = std::str::from_utf8(headers).ok()?;
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(value) = line
                .split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim())
            {
                return value.parse().ok();
            }
        }
        None
    }
}
