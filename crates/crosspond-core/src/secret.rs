use std::fmt;
use std::sync::Arc;

use crosspond_model::{ChatGptOAuthTokens, ChatGptTokenStore, ModelError};
use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, DEFAULT_COMPAT_ID};

/// Identifies a secret in the platform store. Not the secret value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SecretKey {
    pub service: &'static str,
    pub account: String,
}

impl SecretKey {
    pub const SERVICE: &'static str = "com.crosspond.app";

    fn new(account: impl Into<String>) -> Self {
        Self {
            service: Self::SERVICE,
            account: account.into(),
        }
    }

    /// Default Compatible endpoint. Also the legacy Keychain account.
    pub fn provider_api_key() -> Self {
        Self::provider_api_key_for(DEFAULT_COMPAT_ID)
    }

    /// Compatible API key. Id `default` keeps `provider.api_key`; others use `provider.api_key.{id}`.
    pub fn provider_api_key_for(id: &str) -> Self {
        let id = crate::config::sanitize_compat_id(id);
        if id == DEFAULT_COMPAT_ID {
            Self::new("provider.api_key")
        } else {
            Self::new(format!("provider.api_key.{id}"))
        }
    }

    pub fn exa_api_key() -> Self {
        Self::new("exa.api_key")
    }

    pub fn chatgpt_oauth() -> Self {
        Self::new("provider.chatgpt_oauth")
    }

    /// Keychain item for a vault `credential_ref`. Account is `credential.{ref}`.
    pub fn credential(credential_ref: &str) -> Result<Self, SecretError> {
        let credential_ref = parse_credential_ref(credential_ref)?;
        Ok(Self::new(format!("credential.{credential_ref}")))
    }
}

const CREDENTIAL_REF_PATTERN: &str = "lowercase letters, digits, `.`, `_`, or `-`";

/// Validate a Knowledge Vault `credential_ref` pointer (not a secret value).
pub fn parse_credential_ref(value: &str) -> Result<String, SecretError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(SecretError::InvalidRef("credential_ref is required".into()));
    }
    if value.len() > 64 {
        return Err(SecretError::InvalidRef(
            "credential_ref must be at most 64 characters".into(),
        ));
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(SecretError::InvalidRef("credential_ref is required".into()));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(SecretError::InvalidRef(format!(
            "credential_ref must start with a lowercase letter or digit ({CREDENTIAL_REF_PATTERN})"
        )));
    }
    if !chars
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(SecretError::InvalidRef(format!(
            "credential_ref may only contain {CREDENTIAL_REF_PATTERN}"
        )));
    }
    if value == "provider.api_key" || value == "exa.api_key" || value == "provider.chatgpt_oauth" {
        return Err(SecretError::InvalidRef(
            "credential_ref cannot use a reserved Keychain account".into(),
        ));
    }
    Ok(value.to_string())
}

/// Username/password bundle stored as one Keychain item. Never log the JSON.
#[derive(Clone, Deserialize, Serialize)]
pub struct CredentialBundle {
    pub username: String,
    pub password: String,
}

impl CredentialBundle {
    pub fn encode(&self) -> SecretString {
        SecretString::new(serde_json::to_string(self).unwrap_or_else(|_| "{}".into()))
    }

    pub fn decode(secret: &SecretString) -> Result<Self, SecretError> {
        serde_json::from_str(secret.expose()).map_err(|_| {
            SecretError::Backend("stored login was not a username/password bundle".into())
        })
    }
}

impl fmt::Debug for CredentialBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialBundle")
            .field("username", &"***")
            .field("password", &"***")
            .finish()
    }
}

/// Secret bytes that must never be logged or persisted in config.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Expose the secret to a provider HTTP layer or a host-side fill. Do not log the result.
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
    #[error("{0}")]
    InvalidRef(String),
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError>;
    fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError>;
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;
}

pub fn provider_key_is_set(secrets: &dyn SecretStore) -> bool {
    compat_key_is_set(DEFAULT_COMPAT_ID, secrets)
}

pub fn compat_key_is_set(id: &str, secrets: &dyn SecretStore) -> bool {
    secrets
        .get(&SecretKey::provider_api_key_for(id))
        .ok()
        .flatten()
        .is_some_and(|key| !key.is_empty())
}

pub fn any_compat_key_is_set(config: &AppConfig, secrets: &dyn SecretStore) -> bool {
    config
        .openai_compat
        .iter()
        .any(|endpoint| compat_key_is_set(&endpoint.id, secrets))
}

pub fn chatgpt_oauth_is_set(secrets: &dyn SecretStore) -> bool {
    load_chatgpt_tokens(secrets).ok().flatten().is_some()
}

/// Ready for onboarding: ChatGPT signed in, or any Compatible endpoint has a key.
pub fn provider_is_ready(config: &AppConfig, secrets: &dyn SecretStore) -> bool {
    chatgpt_oauth_is_set(secrets) || any_compat_key_is_set(config, secrets)
}

/// Ready to run the currently selected model.
pub fn selected_provider_is_ready(config: &AppConfig, secrets: &dyn SecretStore) -> bool {
    if config.selected.is_chatgpt() {
        chatgpt_oauth_is_set(secrets)
    } else {
        compat_key_is_set(&config.selected.source, secrets)
    }
}

pub fn load_chatgpt_tokens(
    secrets: &dyn SecretStore,
) -> Result<Option<ChatGptOAuthTokens>, SecretError> {
    let Some(blob) = secrets
        .get(&SecretKey::chatgpt_oauth())?
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
    secrets.set(&SecretKey::chatgpt_oauth(), &SecretString::new(json))
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

    fn load(&self) -> Result<Option<ChatGptOAuthTokens>, ModelError> {
        load_chatgpt_tokens(&*self.inner).map_err(|err| ModelError::Network(err.to_string()))
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
    use crate::config::{AppConfig, CHATGPT_SOURCE, OpenaiCompatEndpoint, SelectedModel};

    #[test]
    fn debug_does_not_include_secret() {
        let secret = SecretString::new("sk-live-super-secret");
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "SecretString(..)");
        assert!(!rendered.contains("sk-"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn default_compat_key_uses_legacy_account() {
        assert_eq!(SecretKey::provider_api_key().account, "provider.api_key");
        assert_eq!(
            SecretKey::provider_api_key_for("default").account,
            "provider.api_key"
        );
        assert_eq!(
            SecretKey::provider_api_key_for("compat-2").account,
            "provider.api_key.compat-2"
        );
    }

    #[test]
    fn compat_keys_do_not_mix() {
        let secrets = memory::MemorySecretStore::default();
        secrets
            .set(
                &SecretKey::provider_api_key_for("default"),
                &SecretString::new("sk-default"),
            )
            .unwrap();
        secrets
            .set(
                &SecretKey::provider_api_key_for("compat-2"),
                &SecretString::new("sk-other"),
            )
            .unwrap();
        assert_eq!(
            secrets
                .get(&SecretKey::provider_api_key_for("default"))
                .unwrap()
                .unwrap()
                .expose(),
            "sk-default"
        );
        assert_eq!(
            secrets
                .get(&SecretKey::provider_api_key_for("compat-2"))
                .unwrap()
                .unwrap()
                .expose(),
            "sk-other"
        );
    }

    #[test]
    fn chatgpt_oauth_blob_round_trip_and_readiness() {
        let secrets = memory::MemorySecretStore::default();
        let mut config = AppConfig::default();
        assert!(!provider_is_ready(&config, &secrets));
        config.selected = SelectedModel::chatgpt("gpt-5.6-luna");
        assert!(!provider_is_ready(&config, &secrets));
        assert!(!selected_provider_is_ready(&config, &secrets));
        let tokens = ChatGptOAuthTokens {
            access: "access-secret".into(),
            refresh: "refresh-secret".into(),
            expires_at: 1,
            account_id: "acct".into(),
        };
        save_chatgpt_tokens(&secrets, &tokens).unwrap();
        assert!(chatgpt_oauth_is_set(&secrets));
        assert!(provider_is_ready(&config, &secrets));
        assert!(selected_provider_is_ready(&config, &secrets));
        let loaded = load_chatgpt_tokens(&secrets).unwrap().unwrap();
        assert_eq!(loaded.account_id, "acct");
    }

    #[test]
    fn onboarding_ready_with_compat_key_without_chatgpt() {
        let secrets = memory::MemorySecretStore::default();
        let config = AppConfig::default();
        secrets
            .set(
                &SecretKey::provider_api_key(),
                &SecretString::new("sk-test"),
            )
            .unwrap();
        assert!(provider_is_ready(&config, &secrets));
        assert!(selected_provider_is_ready(&config, &secrets));
        let mut chatgpt = config.clone();
        chatgpt.selected = SelectedModel::chatgpt("gpt-5.6-luna");
        assert!(provider_is_ready(&chatgpt, &secrets));
        assert!(!selected_provider_is_ready(&chatgpt, &secrets));
    }

    #[test]
    fn extra_compat_key_does_not_satisfy_default_selected() {
        let secrets = memory::MemorySecretStore::default();
        let mut config = AppConfig::default();
        config.openai_compat.push(OpenaiCompatEndpoint {
            id: "compat-2".into(),
            name: "Local".into(),
            base_url: "http://127.0.0.1:1234/v1".into(),
        });
        secrets
            .set(
                &SecretKey::provider_api_key_for("compat-2"),
                &SecretString::new("sk-local"),
            )
            .unwrap();
        assert!(provider_is_ready(&config, &secrets));
        assert!(!selected_provider_is_ready(&config, &secrets));
        config.selected = SelectedModel::compat("compat-2", "qwen");
        assert!(selected_provider_is_ready(&config, &secrets));
    }

    #[test]
    fn chatgpt_source_constant_is_stable() {
        assert_eq!(CHATGPT_SOURCE, "chatgpt");
    }

    #[test]
    fn credential_ref_must_be_a_safe_slug() {
        assert!(parse_credential_ref("lab.fileserver").is_ok());
        assert!(parse_credential_ref("a").is_ok());
        assert!(parse_credential_ref("").is_err());
        assert!(parse_credential_ref("Lab.File").is_err());
        assert!(parse_credential_ref("lab/fileserver").is_err());
        assert!(parse_credential_ref(&"a".repeat(65)).is_err());
        assert!(parse_credential_ref("provider.api_key").is_err());
        assert!(parse_credential_ref("exa.api_key").is_err());
        assert!(parse_credential_ref("provider.chatgpt_oauth").is_err());
        let key = SecretKey::credential("lab.fileserver").unwrap();
        assert_eq!(key.service, SecretKey::SERVICE);
        assert_eq!(key.account, "credential.lab.fileserver");
        assert_ne!(key.account, SecretKey::provider_api_key().account);
        assert_ne!(key.account, SecretKey::exa_api_key().account);
        assert_ne!(key.account, SecretKey::chatgpt_oauth().account);
    }

    #[test]
    fn credential_bundle_round_trips_without_debug_leak() {
        let bundle = CredentialBundle {
            username: "labuser".into(),
            password: "hunter2".into(),
        };
        let decoded = CredentialBundle::decode(&bundle.encode()).unwrap();
        assert_eq!(decoded.username, "labuser");
        assert_eq!(decoded.password, "hunter2");
        let rendered = format!("{bundle:?}");
        assert!(!rendered.contains("labuser"));
        assert!(!rendered.contains("hunter2"));
    }
}
