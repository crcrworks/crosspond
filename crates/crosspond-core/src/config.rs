use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    OpenaiCompatible,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenaiCompatible,
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("couldn’t read config: {0}")]
    Io(String),
    #[error("couldn’t parse config.json: {0}")]
    Parse(String),
}

pub trait ConfigStore: Send + Sync {
    fn load(&self) -> Result<AppConfig, ConfigError>;
    fn save(&self, config: &AppConfig) -> Result<(), ConfigError>;
}

pub struct FileConfigStore {
    path: PathBuf,
}

impl FileConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
        Self::new(PathBuf::from(home).join(".crosspond").join("config.json"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ConfigStore for FileConfigStore {
    fn load(&self) -> Result<AppConfig, ConfigError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|err| ConfigError::Parse(err.to_string()))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
            Err(err) => Err(ConfigError::Io(err.to_string())),
        }
    }

    fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| ConfigError::Io(err.to_string()))?;
        }
        let json =
            serde_json::to_string_pretty(config).map_err(|err| ConfigError::Io(err.to_string()))?;
        fs::write(&self.path, json).map_err(|err| ConfigError::Io(err.to_string()))
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use std::sync::Mutex;

    use super::{AppConfig, ConfigError, ConfigStore};

    #[derive(Default)]
    pub struct MemoryConfigStore {
        inner: Mutex<AppConfig>,
    }

    impl ConfigStore for MemoryConfigStore {
        fn load(&self) -> Result<AppConfig, ConfigError> {
            Ok(self.inner.lock().expect("config lock").clone())
        }

        fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
            *self.inner.lock().expect("config lock") = config.clone();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_json_does_not_contain_api_key_field() {
        let dir = std::env::temp_dir().join(format!("crosspond-config-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.json");
        let store = FileConfigStore::new(path.clone());
        store.save(&AppConfig::default()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("api_key"));
        assert!(!text.contains("sk-"));
        let loaded = store.load().unwrap();
        assert_eq!(loaded, AppConfig::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let path = std::env::temp_dir().join(format!(
            "crosspond-missing-{}/config.json",
            uuid::Uuid::new_v4()
        ));
        let store = FileConfigStore::new(path);
        assert_eq!(store.load().unwrap(), AppConfig::default());
    }
}
