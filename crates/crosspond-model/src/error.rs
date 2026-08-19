#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("network: {0}")]
    Network(String),
    #[error("provider: {0}")]
    Provider(String),
    #[error("empty response")]
    EmptyResponse,
}

impl ModelError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Unauthorized => "Couldn’t connect to your AI provider. 401 Unauthorized".into(),
            Self::RateLimited => {
                "The AI provider rate-limited this request. Try again in a moment.".into()
            }
            Self::InvalidRequest(detail) => {
                format!("The AI provider rejected the request. {detail}")
            }
            Self::Network(detail) => format!("Couldn’t reach the AI provider. {detail}"),
            Self::Provider(detail) => format!("The AI provider returned an error. {detail}"),
            Self::EmptyResponse => {
                "The AI provider returned no text. For local servers, Base URL should end with /v1 (e.g. http://127.0.0.1:1234/v1).".into()
            }
        }
    }

    pub fn from_status(status: u16, body: &str) -> Self {
        let snippet = truncate_error_body(body);
        match status {
            401 | 403 => Self::Unauthorized,
            429 => Self::RateLimited,
            400 | 404 | 422 => Self::InvalidRequest(snippet),
            _ => Self::Provider(format!("{status}: {snippet}")),
        }
    }
}

fn truncate_error_body(body: &str) -> String {
    if let Some(message) = provider_error_message(body) {
        return message;
    }
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return "invalid request".into();
    }
    body.chars().filter(|ch| *ch != '\n').take(200).collect()
}

fn provider_error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    json_error_message(&value)
}

fn json_error_message(value: &serde_json::Value) -> Option<String> {
    if let Some(message) = value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(|value| value.as_str())
        .filter(|message| !message.is_empty())
    {
        return Some(message.chars().take(200).collect());
    }
    value
        .as_array()
        .and_then(|items| items.first())
        .and_then(json_error_message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_openai_error_json_without_dumping_body() {
        let err = ModelError::from_status(
            400,
            r#"{"error":{"message":"model not found","type":"invalid_request_error"}}"#,
        );
        match err {
            ModelError::InvalidRequest(message) => {
                assert_eq!(message, "model not found");
                assert!(!message.contains("{"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn maps_array_wrapped_gemini_error_without_dumping_body() {
        let err = ModelError::from_status(
            400,
            r#"[{"error":{"code":400,"message":"Function call is missing a thought_signature in functionCall parts.","status":"INVALID_ARGUMENT"}}]"#,
        );
        match err {
            ModelError::InvalidRequest(message) => {
                assert!(message.contains("thought_signature"));
                assert!(!message.contains('['));
                assert!(!message.contains('{'));
            }
            other => panic!("{other:?}"),
        }
    }
}
