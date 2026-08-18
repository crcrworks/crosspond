use std::path::{Path, PathBuf};

use crosspond_tools::{ScratchError, ScratchReason, ScratchSpace};

use crate::ids::TaskId;

pub trait ScratchSpaceManager: Send + Sync {
    fn ensure(&self, task_id: TaskId, reason: ScratchReason) -> Result<ScratchSpace, ScratchError>;
    fn cleanup(&self, space: &ScratchSpace) -> Result<(), ScratchError>;
}

pub struct FsScratchSpaceManager {
    root: PathBuf,
}

impl FsScratchSpaceManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
        Self::new(PathBuf::from(home).join(".crosspond").join("scratch"))
    }
}

impl ScratchSpaceManager for FsScratchSpaceManager {
    fn ensure(
        &self,
        task_id: TaskId,
        _reason: ScratchReason,
    ) -> Result<ScratchSpace, ScratchError> {
        ScratchSpace::create(self.root.join(task_id.to_string()))
    }

    fn cleanup(&self, space: &ScratchSpace) -> Result<(), ScratchError> {
        let root = canonicalize_or_io(&self.root)?;
        let space_root = canonicalize_or_io(&space.root)?;
        if !is_inside(&root, &space_root) {
            return Err(ScratchError::Io(
                "scratch path is outside the scratch root".into(),
            ));
        }
        std::fs::remove_dir_all(&space_root).map_err(|err| ScratchError::Io(err.to_string()))?;
        Ok(())
    }
}

pub fn default_tasks_root() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home).join(".crosspond").join("tasks")
}

fn canonicalize_or_io(path: &Path) -> Result<PathBuf, ScratchError> {
    path.canonicalize()
        .map_err(|err| ScratchError::Io(err.to_string()))
}

fn is_inside(root: &Path, path: &Path) -> bool {
    let mut current = path;
    loop {
        if current == root {
            return true;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TaskId;

    #[test]
    fn new_does_not_create_root() {
        let root =
            std::env::temp_dir().join(format!("crosspond-scratch-new-{}", uuid::Uuid::new_v4()));
        let _manager = FsScratchSpaceManager::new(root.clone());
        assert!(!root.exists());
    }

    #[test]
    fn ensure_makes_input_work_output() {
        let root = std::env::temp_dir().join(format!("crosspond-scratch-{}", uuid::Uuid::new_v4()));
        let manager = FsScratchSpaceManager::new(root.clone());
        let space = manager
            .ensure(TaskId::new(), ScratchReason::FileProcessing)
            .unwrap();
        assert!(space.input.is_dir());
        assert!(space.work.is_dir());
        assert!(space.output.is_dir());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_removes_scratch_under_root() {
        let root =
            std::env::temp_dir().join(format!("crosspond-scratch-clean-{}", uuid::Uuid::new_v4()));
        let manager = FsScratchSpaceManager::new(root.clone());
        let space = manager
            .ensure(TaskId::new(), ScratchReason::ShellExecution)
            .unwrap();
        assert!(space.root.is_dir());
        manager.cleanup(&space).unwrap();
        assert!(!space.root.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_rejects_path_outside_root() {
        let root =
            std::env::temp_dir().join(format!("crosspond-scratch-out-{}", uuid::Uuid::new_v4()));
        let outside =
            std::env::temp_dir().join(format!("crosspond-scratch-victim-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).unwrap();
        let marker = outside.join("keep.txt");
        std::fs::write(&marker, "keep").unwrap();
        let manager = FsScratchSpaceManager::new(root);
        let fake = ScratchSpace {
            input: outside.join("input"),
            work: outside.join("work"),
            output: outside.join("output"),
            root: outside.clone(),
        };
        assert!(manager.cleanup(&fake).is_err());
        assert!(marker.exists());
        let _ = std::fs::remove_dir_all(outside);
    }
}
