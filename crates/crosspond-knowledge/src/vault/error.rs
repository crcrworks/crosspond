use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum VaultError {
    #[error("vault path is required")]
    EmptyRoot,
    #[error("invalid note id")]
    InvalidId,
    #[error("malformed YAML frontmatter")]
    MalformedFrontmatter,
    #[error("unknown note type: {0}")]
    UnknownKind(String),
    #[error("note `{0}` was not found")]
    NotFound(String),
    #[error("note id `{0}` is duplicated")]
    DuplicateId(String),
    #[error("a note already exists at {0}")]
    DuplicatePath(String),
    #[error("note `{0}` was modified externally")]
    Conflict(String),
    #[error("path is outside the vault")]
    PathEscape,
    #[error("reserved vault path: {0}")]
    ReservedPath(String),
    #[error("couldn’t access vault: {0}")]
    Io(String),
}
