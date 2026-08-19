use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hotkey::LauncherHotkey;
use crate::policy::ComputerApprovalMode;

/// Default Knowledge Vault folder. Not under `~/.crosspond`.
pub const DEFAULT_VAULT_RELATIVE: &str = "Documents/Crosspond";
pub const DEFAULT_CHATGPT_MODEL: &str = "gpt-5.6-luna";
pub const DEFAULT_COMPAT_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_COMPAT_ID: &str = "default";
pub const CHATGPT_SOURCE: &str = "chatgpt";

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenaiCompatEndpoint {
    pub id: String,
    pub name: String,
    pub base_url: String,
}

impl OpenaiCompatEndpoint {
    pub fn openai_default() -> Self {
        Self {
            id: DEFAULT_COMPAT_ID.into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectedModel {
    /// `"chatgpt"` or an OpenAI Compatible endpoint id.
    pub source: String,
    pub model: String,
}

impl SelectedModel {
    pub fn chatgpt(model: impl Into<String>) -> Self {
        Self {
            source: CHATGPT_SOURCE.into(),
            model: model.into(),
        }
    }

    pub fn compat(id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            source: id.into(),
            model: model.into(),
        }
    }

    pub fn is_chatgpt(&self) -> bool {
        self.source == CHATGPT_SOURCE
    }

    pub fn default_compat() -> Self {
        Self::compat(DEFAULT_COMPAT_ID, DEFAULT_COMPAT_MODEL)
    }
}

impl Default for SelectedModel {
    fn default() -> Self {
        Self::default_compat()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "low" => Self::Low,
            "high" => Self::High,
            "xhigh" => Self::Xhigh,
            _ => Self::Medium,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub openai_compat: Vec<OpenaiCompatEndpoint>,
    #[serde(default)]
    pub selected: SelectedModel,
    #[serde(default)]
    pub reasoning_effort: ReasoningEffort,
    #[serde(default)]
    pub computer_approval: ComputerApprovalMode,
    /// Absolute path to an Obsidian-compatible Knowledge Vault.
    /// Unset means Crosspond will not read or write a vault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_path: Option<PathBuf>,
    /// Global shortcut that toggles the launcher. Default is Option+Space.
    #[serde(default)]
    pub launcher_hotkey: LauncherHotkey,
}

impl AppConfig {
    pub fn effective_vault_path(&self) -> Option<PathBuf> {
        match &self.vault_path {
            Some(path) if path.as_os_str().is_empty() => None,
            Some(path) => Some(expand_user_path(&path.to_string_lossy())),
            None => None,
        }
    }

    pub fn selected_model(&self) -> &str {
        &self.selected.model
    }

    pub fn compat(&self, id: &str) -> Option<&OpenaiCompatEndpoint> {
        self.openai_compat.iter().find(|endpoint| endpoint.id == id)
    }

    pub fn add_compat(&mut self) -> &OpenaiCompatEndpoint {
        let id = next_compat_id(&self.openai_compat);
        let n = self.openai_compat.len() + 1;
        self.openai_compat.push(OpenaiCompatEndpoint {
            id,
            name: format!("Compatible {n}"),
            base_url: "https://api.openai.com/v1".into(),
        });
        self.openai_compat.last().expect("just pushed")
    }

    pub fn remove_compat(&mut self, id: &str) -> bool {
        if self.openai_compat.len() <= 1 {
            return false;
        }
        let Some(index) = self
            .openai_compat
            .iter()
            .position(|endpoint| endpoint.id == id)
        else {
            return false;
        };
        self.openai_compat.remove(index);
        if !self.selected.is_chatgpt() && self.selected.source == id {
            let fallback = &self.openai_compat[0];
            self.selected = SelectedModel::compat(&fallback.id, DEFAULT_COMPAT_MODEL);
        }
        true
    }

    pub fn normalize(&mut self) {
        if self.openai_compat.is_empty() {
            self.openai_compat
                .push(OpenaiCompatEndpoint::openai_default());
        }
        for endpoint in &mut self.openai_compat {
            endpoint.id = sanitize_compat_id(&endpoint.id);
            if endpoint.name.trim().is_empty() {
                endpoint.name = "OpenAI".into();
            }
            if endpoint.base_url.trim().is_empty() {
                endpoint.base_url = "https://api.openai.com/v1".into();
            }
        }
        dedupe_compat_ids(&mut self.openai_compat);
        if self.selected.model.trim().is_empty() {
            self.selected.model = if self.selected.is_chatgpt() {
                DEFAULT_CHATGPT_MODEL.into()
            } else {
                DEFAULT_COMPAT_MODEL.into()
            };
        }
        if !self.selected.is_chatgpt()
            && !self
                .openai_compat
                .iter()
                .any(|endpoint| endpoint.id == self.selected.source)
        {
            self.selected.source = self.openai_compat[0].id.clone();
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            openai_compat: vec![OpenaiCompatEndpoint::openai_default()],
            selected: SelectedModel::default_compat(),
            reasoning_effort: ReasoningEffort::Medium,
            computer_approval: ComputerApprovalMode::Manual,
            vault_path: None,
            launcher_hotkey: LauncherHotkey::default(),
        }
    }
}

impl<'de> Deserialize<'de> for AppConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawAppConfig::deserialize(deserializer)?;
        Ok(AppConfig::from_raw(raw))
    }
}

#[derive(Deserialize)]
struct RawAppConfig {
    #[serde(default)]
    openai_compat: Option<Vec<OpenaiCompatEndpoint>>,
    #[serde(default)]
    selected: Option<SelectedModel>,
    #[serde(default)]
    reasoning_effort: Option<ReasoningEffort>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    computer_approval: ComputerApprovalMode,
    #[serde(default)]
    vault_path: Option<PathBuf>,
    #[serde(default)]
    launcher_hotkey: LauncherHotkey,
}

impl AppConfig {
    fn from_raw(raw: RawAppConfig) -> Self {
        let mut config = if raw.openai_compat.is_some() || raw.selected.is_some() {
            Self {
                openai_compat: raw.openai_compat.unwrap_or_default(),
                selected: raw.selected.unwrap_or_default(),
                reasoning_effort: raw.reasoning_effort.unwrap_or_default(),
                computer_approval: raw.computer_approval,
                vault_path: raw.vault_path,
                launcher_hotkey: raw.launcher_hotkey,
            }
        } else {
            migrate_legacy(raw)
        };
        config.normalize();
        config
    }
}

fn migrate_legacy(raw: RawAppConfig) -> AppConfig {
    let endpoint = OpenaiCompatEndpoint {
        id: DEFAULT_COMPAT_ID.into(),
        name: "OpenAI".into(),
        base_url: raw
            .base_url
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "https://api.openai.com/v1".into()),
    };
    let model = raw
        .model
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_COMPAT_MODEL.into());
    let selected = if raw.provider.as_deref() == Some("chatgpt_codex") {
        SelectedModel::chatgpt(if model == DEFAULT_COMPAT_MODEL {
            DEFAULT_CHATGPT_MODEL
        } else {
            model.as_str()
        })
    } else {
        SelectedModel::compat(DEFAULT_COMPAT_ID, model)
    };
    AppConfig {
        openai_compat: vec![endpoint],
        selected,
        reasoning_effort: raw.reasoning_effort.unwrap_or_default(),
        computer_approval: raw.computer_approval,
        vault_path: raw.vault_path,
        launcher_hotkey: raw.launcher_hotkey,
    }
}

pub fn sanitize_compat_id(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect();
    if cleaned.is_empty() || cleaned == CHATGPT_SOURCE {
        DEFAULT_COMPAT_ID.into()
    } else {
        cleaned
    }
}

fn next_compat_id(existing: &[OpenaiCompatEndpoint]) -> String {
    for index in 2.. {
        let id = format!("compat-{index}");
        if !existing.iter().any(|endpoint| endpoint.id == id) {
            return id;
        }
    }
    "compat-2".into()
}

fn dedupe_compat_ids(endpoints: &mut [OpenaiCompatEndpoint]) {
    let mut seen = std::collections::HashSet::new();
    for endpoint in endpoints.iter_mut() {
        let mut candidate = endpoint.id.clone();
        let mut n = 2;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{}-{n}", endpoint.id);
            n += 1;
        }
        endpoint.id = candidate;
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
        assert!(!text.contains("chatgpt_oauth"));
        assert!(!text.contains("access_token"));
        assert!(!text.contains("\"provider\""));
        assert!(text.contains("openai_compat"));
        assert!(text.contains("reasoning_effort"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_chatgpt_config_migrates_selected() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{"provider":"chatgpt_codex","base_url":"https://api.openai.com/v1","model":"gpt-5.2"}"#,
        )
        .unwrap();
        assert!(parsed.selected.is_chatgpt());
        assert_eq!(parsed.selected.model, "gpt-5.2");
        assert_eq!(parsed.openai_compat.len(), 1);
        assert_eq!(parsed.openai_compat[0].id, DEFAULT_COMPAT_ID);
        let text = serde_json::to_string(&parsed).unwrap();
        assert!(text.contains("chatgpt"));
        assert!(!text.contains("chatgpt_codex"));
        assert!(!text.contains("\"provider\""));
        assert!(!text.contains("access"));
        assert!(!text.contains("refresh"));
    }

    #[test]
    fn legacy_chatgpt_default_mini_becomes_luna() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{"provider":"chatgpt_codex","base_url":"https://api.openai.com/v1","model":"gpt-4o-mini"}"#,
        )
        .unwrap();
        assert!(parsed.selected.is_chatgpt());
        assert_eq!(parsed.selected.model, DEFAULT_CHATGPT_MODEL);
    }

    #[test]
    fn legacy_compatible_config_migrates_one_endpoint() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{"provider":"openai_compatible","base_url":"http://127.0.0.1:1234/v1","model":"qwen2.5"}"#,
        )
        .unwrap();
        assert!(!parsed.selected.is_chatgpt());
        assert_eq!(parsed.selected.source, DEFAULT_COMPAT_ID);
        assert_eq!(parsed.selected.model, "qwen2.5");
        assert_eq!(parsed.openai_compat[0].base_url, "http://127.0.0.1:1234/v1");
        assert_eq!(parsed.reasoning_effort, ReasoningEffort::Medium);
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
        assert_eq!(store.load().unwrap().selected.model, DEFAULT_COMPAT_MODEL);
        assert_eq!(AppConfig::default().openai_compat[0].id, DEFAULT_COMPAT_ID);
    }

    #[test]
    fn new_format_round_trips_multiple_compat() {
        let parsed: AppConfig = serde_json::from_str(
            r#"{
                "openai_compat": [
                    {"id":"default","name":"OpenAI","base_url":"https://api.openai.com/v1"},
                    {"id":"local","name":"Local","base_url":"http://127.0.0.1:1234/v1"}
                ],
                "selected":{"source":"local","model":"qwen"},
                "reasoning_effort":"high"
            }"#,
        )
        .unwrap();
        assert_eq!(parsed.openai_compat.len(), 2);
        assert_eq!(parsed.selected.source, "local");
        assert_eq!(parsed.reasoning_effort, ReasoningEffort::High);
        let text = serde_json::to_string(&parsed).unwrap();
        assert!(text.contains("\"local\""));
        assert!(!text.contains("api_key"));
    }

    #[test]
    fn cannot_remove_last_compat_endpoint() {
        let mut config = AppConfig::default();
        assert!(!config.remove_compat(DEFAULT_COMPAT_ID));
        config.add_compat();
        assert_eq!(config.openai_compat.len(), 2);
        let extra = config.openai_compat[1].id.clone();
        assert!(config.remove_compat(&extra));
        assert_eq!(config.openai_compat.len(), 1);
    }
}
