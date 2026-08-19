use std::fmt;

use serde::{Deserialize, Serialize};

/// Identifies a secret in the platform store. Not the secret value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SecretKey {
    pub service: String,
    pub account: String,
}

impl SecretKey {
    pub const SERVICE: &'static str = "com.crosspond.app";

    pub fn provider_api_key() -> Self {
        Self {
            service: Self::SERVICE.into(),
            account: "provider.api_key".into(),
        }
    }

    pub fn exa_api_key() -> Self {
        Self {
            service: Self::SERVICE.into(),
            account: "exa.api_key".into(),
        }
    }

    /// Keychain item for a vault `credential_ref`. Account is `credential.{ref}`.
    pub fn credential(credential_ref: &str) -> Result<Self, SecretError> {
        let credential_ref = parse_credential_ref(credential_ref)?;
        Ok(Self {
            service: Self::SERVICE.into(),
            account: format!("credential.{credential_ref}"),
        })
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
    if value == "provider.api_key" || value == "exa.api_key" {
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
    secrets
        .get(&SecretKey::provider_api_key())
        .ok()
        .flatten()
        .is_some_and(|key| !key.is_empty())
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
                .get(&(key.service.clone(), key.account.clone()))
                .cloned()
                .map(SecretString::new))
        }

        fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
            self.inner.lock().expect("secret store lock").insert(
                (key.service.clone(), key.account.clone()),
                value.expose().to_string(),
            );
            Ok(())
        }

        fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
            self.inner
                .lock()
                .expect("secret store lock")
                .remove(&(key.service.clone(), key.account.clone()));
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
    fn credential_ref_must_be_a_safe_slug() {
        assert!(parse_credential_ref("lab.fileserver").is_ok());
        assert!(parse_credential_ref("a").is_ok());
        assert!(parse_credential_ref("").is_err());
        assert!(parse_credential_ref("Lab.File").is_err());
        assert!(parse_credential_ref("lab/fileserver").is_err());
        assert!(parse_credential_ref(&"a".repeat(65)).is_err());
        assert!(parse_credential_ref("provider.api_key").is_err());
        assert!(parse_credential_ref("exa.api_key").is_err());
        let key = SecretKey::credential("lab.fileserver").unwrap();
        assert_eq!(key.service, SecretKey::SERVICE);
        assert_eq!(key.account, "credential.lab.fileserver");
        assert_ne!(key.account, SecretKey::provider_api_key().account);
        assert_ne!(key.account, SecretKey::exa_api_key().account);
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
