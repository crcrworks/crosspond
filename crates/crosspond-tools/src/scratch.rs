use std::path::PathBuf;

use thiserror::Error;

/// Why a scratch directory was created. Recorded for tests and later cleanup policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScratchReason {
    FileProcessing,
    ShellExecution,
    /// Reserved until a download tool writes into scratch.
    #[allow(dead_code)]
    Download,
    ArtifactGeneration,
}

/// Temporary working directory for a task that actually needs local files or a shell cwd.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScratchSpace {
    pub root: PathBuf,
    pub input: PathBuf,
    pub work: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Error)]
pub enum ScratchError {
    #[error("couldn’t create scratch space: {0}")]
    Io(String),
}

impl ScratchSpace {
    pub fn create(root: PathBuf) -> Result<Self, ScratchError> {
        let space = Self {
            input: root.join("input"),
            work: root.join("work"),
            output: root.join("output"),
            root,
        };
        for dir in [&space.root, &space.input, &space.work, &space.output] {
            std::fs::create_dir_all(dir).map_err(|err| ScratchError::Io(err.to_string()))?;
        }
        Ok(space)
    }
}
