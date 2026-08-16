//! Obsidian-compatible Knowledge Vault. Must not depend on GPUI or `crosspond-core`.

#![deny(unsafe_code)]

pub mod index;
pub mod model;
pub mod vault;

pub use index::{
    IndexSnapshot, IndexedLink, IndexedVault, SearchHit, SearchIndex, index_db_path, vault_index_id,
};
pub use model::{
    KnowledgeId, KnowledgeNote, KnowledgePatch, NewKnowledgeNote, NoteKind, RelationKind,
    Relations, SourceStatus, TrustLevel, WikiLink,
};
pub use vault::{
    FsVaultRepository, VaultError, VaultRepository, VaultWatcher, WatchMode, parse_wikilinks,
};
