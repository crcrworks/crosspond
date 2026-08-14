use std::path::PathBuf;

use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workspace {
    pub root: PathBuf,
    pub input: PathBuf,
    pub work: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("couldn’t create workspace: {0}")]
    Io(String),
}

impl Workspace {
    pub fn create(root: PathBuf) -> Result<Self, WorkspaceError> {
        let workspace = Self {
            input: root.join("input"),
            work: root.join("work"),
            output: root.join("output"),
            root,
        };
        for dir in [
            &workspace.root,
            &workspace.input,
            &workspace.work,
            &workspace.output,
        ] {
            std::fs::create_dir_all(dir).map_err(|err| WorkspaceError::Io(err.to_string()))?;
        }
        Ok(workspace)
    }
}
