use std::fmt;
use std::sync::Arc;

use crosspond_model::{ChatGptOAuthTokens, ChatGptTokenStore, ModelError};

use crate::config::{AppConfig, ProviderKind};

/// Identifies a secret in the platform store. Not the secret value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SecretKey {
    pub service: &'static str,
    pub account: &'static str,
}

impl SecretKey {
    pub const PROVIDER_API_KEY: Self = Self {
        service: "com.crosspond.app",
        account: "provider.api_key",
    };

    pub const EXA_API_KEY: Self = Self {
        service: "com.crosspond.app",
        account: "exa.api_key",
    };

    pub const CHATGPT_OAUTH: Self = Self {
        service: "com.crosspond.app",
        account: "provider.chatgpt_oauth",
    };
}

/// Secret bytes that must never be logged or persisted in config.
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Expose the secret to a provider HTTP layer. Do not log the result.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(..)")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("couldn’t access the keychain: {0}")]
    Backend(String),
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError>;
    fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError>;
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;
}

pub fn provider_key_is_set(secrets: &dyn SecretStore) -> bool {
    secrets
        .get(&SecretKey::PROVIDER_API_KEY)
        .ok()
        .flatten()
        .is_some_and(|key| !key.is_empty())
}

pub fn chatgpt_oauth_is_set(secrets: &dyn SecretStore) -> bool {
    load_chatgpt_tokens(secrets).ok().flatten().is_some()
}

pub fn provider_is_ready(config: &AppConfig, secrets: &dyn SecretStore) -> bool {
    match config.provider {
        ProviderKind::OpenaiCompatible => provider_key_is_set(secrets),
        ProviderKind::ChatGptCodex => chatgpt_oauth_is_set(secrets),
    }
}

pub fn load_chatgpt_tokens(
    secrets: &dyn SecretStore,
) -> Result<Option<ChatGptOAuthTokens>, SecretError> {
    let Some(blob) = secrets
        .get(&SecretKey::CHATGPT_OAUTH)?
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    match ChatGptOAuthTokens::from_secret_json(blob.expose()) {
        Ok(tokens) => Ok(Some(tokens)),
        Err(_) => Err(SecretError::Backend(
            "stored ChatGPT session is unreadable".into(),
        )),
    }
}

pub fn save_chatgpt_tokens(
    secrets: &dyn SecretStore,
    tokens: &ChatGptOAuthTokens,
) -> Result<(), SecretError> {
    let json = tokens
        .to_secret_json()
        .map_err(|err| SecretError::Backend(err.to_string()))?;
    secrets.set(&SecretKey::CHATGPT_OAUTH, &SecretString::new(json))
}

pub struct SecretChatGptTokenStore {
    inner: Arc<dyn SecretStore>,
}

impl SecretChatGptTokenStore {
    pub fn new(inner: Arc<dyn SecretStore>) -> Self {
        Self { inner }
    }
}

impl ChatGptTokenStore for SecretChatGptTokenStore {
    fn save(&self, tokens: &ChatGptOAuthTokens) -> Result<(), ModelError> {
        save_chatgpt_tokens(&*self.inner, tokens)
            .map_err(|err| ModelError::Network(err.to_string()))
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::{SecretError, SecretKey, SecretStore, SecretString};

    #[derive(Default)]
    pub struct MemorySecretStore {
        inner: Mutex<HashMap<(String, String), String>>,
    }

    impl SecretStore for MemorySecretStore {
        fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError> {
            let map = self.inner.lock().expect("secret store lock");
            Ok(map
                .get(&(key.service.to_string(), key.account.to_string()))
                .cloned()
                .map(SecretString::new))
        }

        fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
            self.inner.lock().expect("secret store lock").insert(
                (key.service.to_string(), key.account.to_string()),
                value.expose().to_string(),
            );
            Ok(())
        }

        fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
            self.inner
                .lock()
                .expect("secret store lock")
                .remove(&(key.service.to_string(), key.account.to_string()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_include_secret() {
        let secret = SecretString::new("sk-live-super-secret");
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "SecretString(..)");
        assert!(!rendered.contains("sk-"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn chatgpt_oauth_blob_round_trip_and_readiness() {
        use crate::config::AppConfig;
        use crate::config::ProviderKind;
        use crosspond_model::ChatGptOAuthTokens;

        let secrets = memory::MemorySecretStore::default();
        let mut config = AppConfig::default();
        assert!(!provider_is_ready(&config, &secrets));
        config.provider = ProviderKind::ChatGptCodex;
        assert!(!provider_is_ready(&config, &secrets));
        let tokens = ChatGptOAuthTokens {
            access: "access-secret".into(),
            refresh: "refresh-secret".into(),
            expires_at: 1,
            account_id: "acct".into(),
        };
        save_chatgpt_tokens(&secrets, &tokens).unwrap();
        assert!(chatgpt_oauth_is_set(&secrets));
        assert!(provider_is_ready(&config, &secrets));
        let loaded = load_chatgpt_tokens(&secrets).unwrap().unwrap();
        assert_eq!(loaded.account_id, "acct");
    }
}
