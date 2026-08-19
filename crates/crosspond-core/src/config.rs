use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hotkey::LauncherHotkey;
use crate::policy::ComputerApprovalMode;

/// Default Knowledge Vault folder. Not under `~/.crosspond`.
pub const DEFAULT_VAULT_RELATIVE: &str = "Documents/Crosspond";

pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
}

/// `~/Documents/Crosspond`.
pub fn default_vault_path() -> PathBuf {
    home_dir().join(DEFAULT_VAULT_RELATIVE)
}

/// Expand `~` / `~/…` and make relative paths sit under `$HOME`.
pub fn expand_user_path(path: &str) -> PathBuf {
    let path = path.trim();
    if path.is_empty() {
        return PathBuf::new();
    }
    let expanded = if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        home_dir().join(expanded)
    }
}

/// Settings input: empty uses the default vault path.
pub fn parse_vault_path_input(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        default_vault_path()
    } else {
        expand_user_path(trimmed)
    }
}

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
    #[serde(default)]
    pub computer_approval: ComputerApprovalMode,
    /// Absolute path to an Obsidian-compatible Knowledge Vault.
    /// Unset means Crosspond will not read or write a vault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_path: Option<PathBuf>,
    /// Global shortcut that toggles the launcher. Default is Option+Space.
    #[serde(default)]
    pub launcher_hotkey: LauncherHotkey,
    /// eTLD+1 hosts the user has Allowed for Chromium browser tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub browser_allowed_hosts: Vec<String>,
    /// Hosts Chromium browser tools must refuse.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub browser_blocked_hosts: Vec<String>,
}

impl AppConfig {
    pub fn effective_vault_path(&self) -> Option<PathBuf> {
        match &self.vault_path {
            Some(path) if path.as_os_str().is_empty() => None,
            Some(path) => Some(expand_user_path(&path.to_string_lossy())),
            None => None,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenaiCompatible,
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            computer_approval: ComputerApprovalMode::Manual,
            vault_path: None,
            launcher_hotkey: LauncherHotkey::default(),
            browser_allowed_hosts: Vec::new(),
            browser_blocked_hosts: Vec::new(),
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
        assert_eq!(loaded.computer_approval, ComputerApprovalMode::Manual);
        assert_eq!(loaded.launcher_hotkey, LauncherHotkey::default());
        assert!(loaded.vault_path.is_none());
        assert!(!text.contains("vault_path"));
        assert!(loaded.browser_allowed_hosts.is_empty());
        assert!(loaded.browser_blocked_hosts.is_empty());
        assert!(!text.contains("browser_allowed_hosts"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_computer_approval_defaults_to_manual() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{"provider":"openai_compatible","base_url":"https://api.openai.com/v1","model":"gpt-4o-mini"}"#,
        )
        .unwrap();
        assert_eq!(parsed.computer_approval, ComputerApprovalMode::Manual);
        assert_eq!(parsed.launcher_hotkey, LauncherHotkey::default());
        assert!(parsed.vault_path.is_none());
    }

    #[test]
    fn launcher_hotkey_is_configurable() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{"provider":"openai_compatible","base_url":"https://api.openai.com/v1","model":"gpt-4o-mini","launcher_hotkey":"control+shift+Space"}"#,
        )
        .unwrap();
        assert_eq!(parsed.launcher_hotkey.to_spec(), "shift+control+Space");
        assert_eq!(
            parsed.launcher_hotkey.display_tokens(),
            ["Control", "Shift", "Space"]
        );
    }

    #[test]
    fn vault_path_is_configurable() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{"provider":"openai_compatible","base_url":"https://api.openai.com/v1","model":"gpt-4o-mini","vault_path":"/Users/example/Documents/Crosspond"}"#,
        )
        .unwrap();
        assert_eq!(
            parsed.vault_path.as_deref(),
            Some(Path::new("/Users/example/Documents/Crosspond"))
        );
        assert_eq!(
            parsed.effective_vault_path().as_deref(),
            Some(Path::new("/Users/example/Documents/Crosspond"))
        );
    }

    #[test]
    fn default_vault_path_is_documents_crosspond() {
        let home = home_dir();
        assert_eq!(
            default_vault_path(),
            home.join("Documents").join("Crosspond")
        );
        assert_eq!(parse_vault_path_input(""), default_vault_path());
        assert_eq!(parse_vault_path_input("   "), default_vault_path());
        assert_eq!(
            parse_vault_path_input("~/Documents/Notes"),
            home.join("Documents").join("Notes")
        );
        assert_eq!(
            parse_vault_path_input("Documents/TeamVault"),
            home.join("Documents").join("TeamVault")
        );
        assert_eq!(
            parse_vault_path_input("/tmp/custom-vault"),
            PathBuf::from("/tmp/custom-vault")
        );
    }

    #[test]
    fn empty_configured_vault_path_is_disabled() {
        let config = AppConfig {
            vault_path: Some(PathBuf::new()),
            ..AppConfig::default()
        };
        assert!(config.effective_vault_path().is_none());
        assert!(AppConfig::default().effective_vault_path().is_none());
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

    #[test]
    fn browser_hosts_roundtrip_without_secrets() {
        let dir = std::env::temp_dir().join(format!("crosspond-hosts-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.json");
        let store = FileConfigStore::new(path.clone());
        let config = AppConfig {
            browser_allowed_hosts: vec!["example.com".into()],
            browser_blocked_hosts: vec!["ads.example".into()],
            ..AppConfig::default()
        };
        store.save(&config).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("example.com"));
        assert!(text.contains("ads.example"));
        assert!(!text.contains("api_key"));
        assert!(!text.contains("sk-"));
        let loaded = store.load().unwrap();
        assert_eq!(loaded.browser_allowed_hosts, vec!["example.com"]);
        assert_eq!(loaded.browser_blocked_hosts, vec!["ads.example"]);
        let _ = fs::remove_dir_all(dir);
    }
}
