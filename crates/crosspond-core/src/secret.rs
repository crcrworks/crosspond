use std::fmt;

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
}
