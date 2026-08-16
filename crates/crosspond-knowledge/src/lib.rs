//! Obsidian-compatible Knowledge Vault. Must not depend on GPUI or `crosspond-core`.

#![deny(unsafe_code)]

pub mod model;
pub mod vault;

pub use model::{
    KnowledgeId, KnowledgeNote, KnowledgePatch, NewKnowledgeNote, NoteKind, RelationKind,
    Relations, SourceStatus, TrustLevel, WikiLink,
};
pub use vault::{FsVaultRepository, VaultError, VaultRepository, parse_wikilinks};
