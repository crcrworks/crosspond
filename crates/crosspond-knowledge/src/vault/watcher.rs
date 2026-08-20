use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};

use super::VaultError;
use super::parse_markdown;
use super::paths::is_indexable_markdown;
use crate::index::SearchIndex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchMode {
    Native,
    Poll { interval: Duration },
}

pub struct VaultWatcher {
    stop: Arc<AtomicBool>,
    _watcher: WatchBackend,
    thread: Option<JoinHandle<()>>,
}

enum WatchBackend {
    Native(RecommendedWatcher),
    Poll(PollWatcher),
}

impl VaultWatcher {
    pub fn start(
        vault_root: PathBuf,
        index: Arc<SearchIndex>,
        debounce: Duration,
        mode: WatchMode,
    ) -> Result<Self, VaultError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = match mode {
            WatchMode::Native => WatchBackend::Native(
                recommended_watcher(tx).map_err(|err| VaultError::Index(err.to_string()))?,
            ),
            WatchMode::Poll { interval } => WatchBackend::Poll(
                PollWatcher::new(
                    tx,
                    Config::default()
                        .with_poll_interval(interval)
                        .with_compare_contents(true),
                )
                .map_err(|err| VaultError::Index(err.to_string()))?,
            ),
        };
        watcher
            .watch(&vault_root, RecursiveMode::Recursive)
            .map_err(|err| VaultError::Index(err.to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("crosspond-vault-watch".into())
            .spawn(move || debounce_loop(vault_root, index, rx, thread_stop, debounce))
            .map_err(|err| VaultError::Index(err.to_string()))?;
        Ok(Self {
            stop,
            _watcher: watcher,
            thread: Some(thread),
        })
    }
}

impl Drop for VaultWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl WatchBackend {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.watch(path, mode),
            Self::Poll(watcher) => watcher.watch(path, mode),
        }
    }
}

fn debounce_loop(
    root: PathBuf,
    index: Arc<SearchIndex>,
    rx: mpsc::Receiver<notify::Result<Event>>,
    stop: Arc<AtomicBool>,
    debounce: Duration,
) {
    let mut pending = HashSet::new();
    let mut batch_started: Option<Instant> = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match rx.recv_timeout(debounce) {
            Ok(Ok(event)) => {
                if skip_kind(&event.kind) {
                    continue;
                }
                if pending.is_empty() {
                    batch_started = Some(Instant::now());
                }
                pending.extend(event.paths);
                if batch_started.is_some_and(|started| started.elapsed() >= debounce) {
                    pending.clear();
                    batch_started = None;
                    reindex_vault(&root, &index);
                }
            }
            Ok(Err(_)) => {}
            Err(RecvTimeoutError::Timeout) => {
                pending.clear();
                batch_started = None;
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                reindex_vault(&root, &index);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn skip_kind(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Access(_))
}

fn reindex_vault(root: &Path, index: &SearchIndex) {
    let mut notes = Vec::new();
    collect_notes(root, root, &mut notes);
    let _ = index.rebuild(&notes);
}

fn collect_notes(root: &Path, dir: &Path, out: &mut Vec<crate::model::KnowledgeNote>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "_system" {
                continue;
            }
            collect_notes(root, &path, out);
        } else {
            let Ok(canonical) = path.canonicalize() else {
                continue;
            };
            if !is_indexable_markdown(root, &canonical) {
                continue;
            }
            let Ok(bytes) = fs::read(&canonical) else {
                continue;
            };
            if let Ok(note) = parse_markdown(root, &canonical, &bytes) {
                out.push(note);
            }
        }
    }
}

fn recommended_watcher(
    tx: mpsc::Sender<notify::Result<Event>>,
) -> notify::Result<RecommendedWatcher> {
    notify::recommended_watcher(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::SearchIndex;
    use crate::vault::{FsVaultRepository, VaultRepository};
    use std::time::Instant;

    fn wait_for(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if check() {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        check()
    }

    #[test]
    fn poll_watcher_indexes_manual_create_and_edit() {
        let id = uuid::Uuid::now_v7();
        let root = std::env::temp_dir().join(format!("crosspond-watch-{id}"));
        let sqlite = std::env::temp_dir().join(format!("crosspond-watch-{id}.sqlite"));
        let repo = FsVaultRepository::open(&root).unwrap();
        let index = Arc::new(SearchIndex::open(&sqlite).unwrap());
        index.rebuild(&repo.list_notes().unwrap()).unwrap();
        let _watch = VaultWatcher::start(
            repo.root().to_path_buf(),
            Arc::clone(&index),
            Duration::from_millis(40),
            WatchMode::Poll {
                interval: Duration::from_millis(20),
            },
        )
        .unwrap();
        fs::write(
            root.join("resources/Lab Wiki.md"),
            "# Lab Wiki\n\nManual Obsidian note.\n",
        )
        .unwrap();
        assert!(wait_for(Duration::from_secs(3), || {
            index
                .search("Lab Wiki", 10)
                .unwrap()
                .iter()
                .any(|hit| hit.title == "Lab Wiki")
        }));
        fs::write(
            root.join("resources/Lab Wiki.md"),
            "# Lab Wiki\n\nUpdated assignment page.\n",
        )
        .unwrap();
        assert!(wait_for(Duration::from_secs(3), || {
            index
                .search("assignment page", 10)
                .unwrap()
                .iter()
                .any(|hit| hit.title == "Lab Wiki")
        }));
        drop(_watch);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn poll_watcher_updates_path_after_rename() {
        use crate::index::IndexedVault;
        use crate::model::{NewKnowledgeNote, NoteKind, Relations, TrustLevel};

        let id = uuid::Uuid::now_v7();
        let root = std::env::temp_dir().join(format!("crosspond-watch-rename-{id}"));
        let sqlite = std::env::temp_dir().join(format!("crosspond-watch-rename-{id}.sqlite"));
        let indexed = IndexedVault::open(&root, &sqlite).unwrap();
        indexed
            .create_note(NewKnowledgeNote {
                kind: NoteKind::Resource,
                title: "Lab VPN".into(),
                aliases: Vec::new(),
                tags: Vec::new(),
                trust: TrustLevel::User,
                relations: Relations::default(),
                resource_kind: Some("vpn".into()),
                credential_ref: None,
                body: "# Lab VPN\n".into(),
                relative_path: None,
                url: None,
                source_kind: None,
                source_status: None,
            })
            .unwrap();
        let _watch = indexed
            .watch(
                Duration::from_millis(40),
                WatchMode::Poll {
                    interval: Duration::from_millis(20),
                },
            )
            .unwrap();
        fs::rename(
            root.join("resources/Lab VPN.md"),
            root.join("resources/Laboratory VPN.md"),
        )
        .unwrap();
        assert!(wait_for(Duration::from_secs(3), || {
            indexed
                .search("Lab VPN", 10)
                .unwrap()
                .iter()
                .any(|hit| hit.path.as_path() == Path::new("resources/Laboratory VPN.md"))
        }));
        drop(_watch);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(sqlite);
    }
}
