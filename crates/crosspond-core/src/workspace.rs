use std::path::PathBuf;

use crosspond_tools::{Workspace, WorkspaceError};

use crate::ids::TaskId;

pub trait WorkspaceManager: Send + Sync {
    fn create(&self, task_id: TaskId) -> Result<Workspace, WorkspaceError>;
}

pub struct FsWorkspaceManager {
    root: PathBuf,
}

impl FsWorkspaceManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
        Self::new(PathBuf::from(home).join(".crosspond").join("workspaces"))
    }
}

impl WorkspaceManager for FsWorkspaceManager {
    fn create(&self, task_id: TaskId) -> Result<Workspace, WorkspaceError> {
        Workspace::create(self.root.join(task_id.to_string()))
    }
}

pub fn default_tasks_root() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home).join(".crosspond").join("tasks")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TaskId;

    #[test]
    fn create_makes_input_work_output() {
        let root = std::env::temp_dir().join(format!("crosspond-ws-{}", uuid::Uuid::new_v4()));
        let manager = FsWorkspaceManager::new(root.clone());
        let workspace = manager.create(TaskId::new()).unwrap();
        assert!(workspace.input.is_dir());
        assert!(workspace.work.is_dir());
        assert!(workspace.output.is_dir());
        let _ = std::fs::remove_dir_all(root);
    }
}
