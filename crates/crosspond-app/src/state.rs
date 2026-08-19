use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use crosspond_core::{
    CommandSender, ConfigStore, ContextCapsule, ContextCollector, ConversationId,
    GlobalHotkeyService, SecretStore, TaskId,
};

pub struct PendingChatGptLogin {
    pub verifier: String,
    pub state: String,
}

pub struct AppState {
    pub commands: CommandSender,
    pub config: Arc<dyn ConfigStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub collector: Arc<dyn ContextCollector>,
    pub hotkey: Mutex<Box<dyn GlobalHotkeyService>>,
    pub inner: Mutex<InnerState>,
    pub pending_chatgpt: Mutex<Option<PendingChatGptLogin>>,
    pub models_cache: Mutex<Option<ModelsCacheEntry>>,
    _runtime: JoinHandle<()>,
}

pub struct ModelsCacheEntry {
    pub at: Instant,
    pub catalog: crate::commands::ModelsCatalog,
}

pub struct InnerState {
    pub ambient: ContextCapsule,
    pub current_task: Option<TaskId>,
    pub conversation_id: Option<ConversationId>,
    pub artifacts: Vec<(String, PathBuf)>,
    pub visible: bool,
    pub in_conversation: bool,
    pub compact: bool,
    pub composing: bool,
    /// Latest launcher resize request. Older queued resizes are dropped.
    pub resize_seq: u64,
    /// Settings is capturing a new shortcut; ignore launcher toggles.
    pub capturing_hotkey: bool,
}

impl AppState {
    pub fn new(
        commands: CommandSender,
        config: Arc<dyn ConfigStore>,
        secrets: Arc<dyn SecretStore>,
        collector: Arc<dyn ContextCollector>,
        hotkey: Box<dyn GlobalHotkeyService>,
        runtime: JoinHandle<()>,
    ) -> Self {
        Self {
            commands,
            config,
            secrets,
            collector,
            hotkey: Mutex::new(hotkey),
            inner: Mutex::new(InnerState {
                ambient: ContextCapsule::default(),
                current_task: None,
                conversation_id: None,
                artifacts: Vec::new(),
                visible: false,
                in_conversation: false,
                compact: true,
                composing: false,
                resize_seq: 0,
                capturing_hotkey: false,
            }),
            pending_chatgpt: Mutex::new(None),
            models_cache: Mutex::new(None),
            _runtime: runtime,
        }
    }

    pub fn lock_inner(&self) -> std::sync::MutexGuard<'_, InnerState> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn lock_hotkey(&self) -> std::sync::MutexGuard<'_, Box<dyn GlobalHotkeyService>> {
        self.hotkey.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn lock_pending_chatgpt(&self) -> std::sync::MutexGuard<'_, Option<PendingChatGptLogin>> {
        self.pending_chatgpt
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    pub fn invalidate_models(&self) {
        *self
            .models_cache
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = None;
    }
}

impl InnerState {
    pub fn bump_resize_seq(&mut self) -> u64 {
        self.resize_seq = self.resize_seq.wrapping_add(1);
        self.resize_seq
    }
}

pub struct NoopHotkey;

impl GlobalHotkeyService for NoopHotkey {
    fn poll(&self) -> Option<crosspond_core::HotkeyEvent> {
        None
    }

    fn set_hotkey(&mut self, _spec: &crosspond_core::LauncherHotkey) -> Result<(), String> {
        Err("global hotkeys are not available".into())
    }

    fn clear_hotkey(&mut self) -> Result<(), String> {
        Ok(())
    }
}
