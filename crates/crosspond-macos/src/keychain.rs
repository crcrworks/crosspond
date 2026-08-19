use crosspond_core::{SecretError, SecretKey, SecretStore, SecretString};

/// Keychain-backed [`SecretStore`]. Binding details stay in this crate.
pub struct MacOsKeychainSecretStore;

#[cfg(target_os = "macos")]
fn map_backend(err: security_framework::base::Error) -> SecretError {
    SecretError::Backend(err.to_string())
}

impl SecretStore for MacOsKeychainSecretStore {
    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = key;
            Err(SecretError::Backend(
                "Keychain is only available on macOS".into(),
            ))
        }

        #[cfg(target_os = "macos")]
        {
            use security_framework::passwords::{PasswordOptions, generic_password};
            use security_framework_sys::base::errSecItemNotFound;

            match generic_password(PasswordOptions::new_generic_password(
                &key.service,
                &key.account,
            )) {
                Ok(bytes) => {
                    let value = String::from_utf8(bytes)
                        .map_err(|_| SecretError::Backend("keychain entry was not UTF-8".into()))?;
                    if value.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(SecretString::new(value)))
                    }
                }
                Err(err) if err.code() == errSecItemNotFound => Ok(None),
                Err(err) => Err(map_backend(err)),
            }
        }
    }

    fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (key, value);
            Err(SecretError::Backend(
                "Keychain is only available on macOS".into(),
            ))
        }

        #[cfg(target_os = "macos")]
        {
            security_framework::passwords::set_generic_password(
                &key.service,
                &key.account,
                value.expose().as_bytes(),
            )
            .map_err(map_backend)
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = key;
            Err(SecretError::Backend(
                "Keychain is only available on macOS".into(),
            ))
        }

        #[cfg(target_os = "macos")]
        {
            use security_framework_sys::base::errSecItemNotFound;

            match security_framework::passwords::delete_generic_password(&key.service, &key.account)
            {
                Ok(()) => Ok(()),
                Err(err) if err.code() == errSecItemNotFound => Ok(()),
                Err(err) => Err(map_backend(err)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crosspond_core::SecretKey;

    #[cfg(target_os = "macos")]
    #[test]
    fn missing_item_is_none_not_an_error() {
        let store = MacOsKeychainSecretStore;
        let key = SecretKey {
            service: "com.crosspond.app.test".into(),
            account: "missing.item.should.not.exist".into(),
        };
        let _ = store.delete(&key);
        let value = store.get(&key).expect("get");
        assert!(value.is_none());
    }
}
