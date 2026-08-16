use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathScope {
    /// Inside the task scratch space.
    Workspace,
    External,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPath {
    pub path: PathBuf,
    pub scope: PathScope,
}

#[derive(Debug, Error)]
pub enum PathError {
    #[error("path is required")]
    Empty,
    #[error("no scratch space is available")]
    NoScratch,
    #[error("couldn’t resolve path: {0}")]
    Io(String),
}

/// Resolve `requested` against `scratch_root` and classify it.
///
/// Membership uses canonical paths and parent walking, not `Path::starts_with`.
pub fn resolve_path(scratch_root: &Path, requested: &str) -> Result<ResolvedPath, PathError> {
    resolve_requested(Some(scratch_root), requested)
}

/// Resolve a tool path when a scratch space may not exist yet.
///
/// Relative paths require a scratch root. Absolute paths are always External
/// when no scratch root is provided.
pub fn resolve_requested(
    scratch_root: Option<&Path>,
    requested: &str,
) -> Result<ResolvedPath, PathError> {
    let requested = requested.trim();
    if requested.is_empty() {
        return Err(PathError::Empty);
    }
    let raw = Path::new(requested);
    let Some(scratch_root) = scratch_root else {
        if !raw.is_absolute() {
            return Err(PathError::NoScratch);
        }
        let path = resolve_components(Path::new("/"), raw)?;
        return Ok(ResolvedPath {
            path,
            scope: PathScope::External,
        });
    };
    let scratch_root = scratch_root
        .canonicalize()
        .map_err(|err| PathError::Io(err.to_string()))?;
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        scratch_root.join(raw)
    };
    let path = resolve_components(&scratch_root, &joined)?;
    let scope = if is_inside(&scratch_root, &path) {
        PathScope::Workspace
    } else {
        PathScope::External
    };
    Ok(ResolvedPath { path, scope })
}

pub fn classify_write_path(scratch_root: &Path, requested: &str) -> Result<PathScope, PathError> {
    Ok(resolve_path(scratch_root, requested)?.scope)
}

fn resolve_components(workspace: &Path, joined: &Path) -> Result<PathBuf, PathError> {
    let mut current = if joined.is_absolute() {
        PathBuf::from(Component::RootDir.as_os_str())
    } else {
        workspace.to_path_buf()
    };

    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => {
                current = PathBuf::from(prefix.as_os_str());
            }
            Component::RootDir => {
                current = PathBuf::from(Component::RootDir.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                current.pop();
            }
            Component::Normal(name) => {
                let next = current.join(name);
                if next.exists() {
                    current = next
                        .canonicalize()
                        .map_err(|err| PathError::Io(err.to_string()))?;
                } else {
                    current = next;
                }
            }
        }
    }
    Ok(current)
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
    use std::fs;
    use std::os::unix::fs::symlink;

    use uuid::Uuid;

    fn setup_workspace() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("crosspond-path-{}", Uuid::new_v4()));
        fs::create_dir_all(dir.join("work")).unwrap();
        fs::create_dir_all(dir.join("output")).unwrap();
        dir
    }

    #[test]
    fn nested_workspace_path_is_workspace() {
        let root = setup_workspace();
        let resolved = resolve_path(&root, "output/notes/hello.txt").unwrap();
        assert_eq!(resolved.scope, PathScope::Workspace);
        assert!(resolved.path.ends_with("output/notes/hello.txt"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parent_dir_escape_is_external() {
        let root = setup_workspace();
        let resolved = resolve_path(&root, "../secret.txt").unwrap();
        assert_eq!(resolved.scope, PathScope::External);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn absolute_outside_path_is_external() {
        let root = setup_workspace();
        let desktop = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()) + "/Desktop/file.txt";
        let resolved = resolve_path(&root, &desktop).unwrap();
        assert_eq!(resolved.scope, PathScope::External);
        assert_eq!(
            classify_write_path(&root, &desktop).unwrap(),
            PathScope::External
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn symlink_escape_is_external() {
        let root = setup_workspace();
        let outside = std::env::temp_dir().join(format!("crosspond-out-{}", Uuid::new_v4()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "nope").unwrap();
        symlink(&outside, root.join("link")).unwrap();
        let resolved = resolve_path(&root, "link/secret.txt").unwrap();
        assert_eq!(resolved.scope, PathScope::External);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn empty_path_errors() {
        let root = setup_workspace();
        assert!(matches!(resolve_path(&root, "  "), Err(PathError::Empty)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn relative_path_without_scratch_errors() {
        assert!(matches!(
            resolve_requested(None, "output/hello.txt"),
            Err(PathError::NoScratch)
        ));
    }

    #[test]
    fn absolute_path_without_scratch_is_external() {
        let resolved = resolve_requested(None, "/tmp/crosspond-no-scratch.txt").unwrap();
        assert_eq!(resolved.scope, PathScope::External);
        assert!(resolved.path.ends_with("crosspond-no-scratch.txt"));
    }
}
