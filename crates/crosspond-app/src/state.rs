use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crosspond_core::{
    CommandSender, ConfigStore, ContextCapsule, ContextCollector, ConversationId,
    GlobalHotkeyService, SecretStore, TaskId,
};
use crosspond_tools::AppBackend;

pub struct AppState {
    pub commands: CommandSender,
    pub config: Arc<dyn ConfigStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub collector: Arc<dyn ContextCollector>,
    pub apps: Arc<dyn AppBackend>,
    pub hotkey: Mutex<Box<dyn GlobalHotkeyService>>,
    pub inner: Mutex<InnerState>,
    _runtime: JoinHandle<()>,
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
}

impl AppState {
    pub fn new(
        commands: CommandSender,
        config: Arc<dyn ConfigStore>,
        secrets: Arc<dyn SecretStore>,
        collector: Arc<dyn ContextCollector>,
        apps: Arc<dyn AppBackend>,
        hotkey: Box<dyn GlobalHotkeyService>,
        runtime: JoinHandle<()>,
    ) -> Self {
        Self {
            commands,
            config,
            secrets,
            collector,
            apps,
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
            }),
            _runtime: runtime,
        }
    }

    pub fn lock_inner(&self) -> std::sync::MutexGuard<'_, InnerState> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn lock_hotkey(&self) -> std::sync::MutexGuard<'_, Box<dyn GlobalHotkeyService>> {
        self.hotkey.lock().unwrap_or_else(|err| err.into_inner())
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
}
