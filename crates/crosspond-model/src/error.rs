#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited,
    #[error("usage limited")]
    UsageLimited,
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
            Self::UsageLimited => {
                "ChatGPT usage limit reached. Try again after the 5-hour or weekly window resets."
                    .into()
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
        if looks_like_usage_limit(status, body) {
            return Self::UsageLimited;
        }
        match status {
            401 | 403 => Self::Unauthorized,
            429 => Self::RateLimited,
            400 | 404 | 422 => Self::InvalidRequest(snippet),
            _ => Self::Provider(format!("{status}: {snippet}")),
        }
    }
}

fn looks_like_usage_limit(status: u16, body: &str) -> bool {
    if status != 404 && status != 429 {
        return false;
    }
    let haystack = body.to_ascii_lowercase();
    haystack.contains("usage_limit_reached")
        || haystack.contains("usage_not_included")
        || haystack.contains("usage limit")
}

fn truncate_error_body(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(message) = value
            .pointer("/error/message")
            .or_else(|| value.get("message"))
            .and_then(|value| value.as_str())
    {
        return message.chars().take(200).collect();
    }
    body.chars().filter(|ch| *ch != '\n').take(200).collect()
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
    fn maps_chatgpt_usage_limit_without_dumping_body() {
        let err = ModelError::from_status(
            404,
            r#"{"error":{"code":"usage_limit_reached","message":"quota exhausted until Friday"}}"#,
        );
        assert!(matches!(err, ModelError::UsageLimited));
        assert!(err.user_message().contains("5-hour"));
        assert!(!err.user_message().contains("Friday"));
        assert!(!err.user_message().contains("quota exhausted"));
    }
}
