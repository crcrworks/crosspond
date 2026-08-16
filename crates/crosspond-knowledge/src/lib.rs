//! Obsidian-compatible Knowledge Vault. Must not depend on GPUI or `crosspond-core`.

#![deny(unsafe_code)]

pub mod activity;
pub mod index;
pub mod ingest;
pub mod model;
pub mod retrieval;
pub mod vault;

pub use activity::{ActivityRecord, ActivityRecorder, ActivityStatus, parse_note_id};
pub use index::{
    IndexSnapshot, IndexedLink, IndexedVault, SearchHit, SearchIndex, index_db_path, index_note_id,
    vault_index_id,
};
pub use ingest::{
    IngestionEngine, IngestionOutcome, IngestionPlan, SourceCapture, looks_like_secret,
};
pub use model::{
    KnowledgeId, KnowledgeNote, KnowledgePatch, NewKnowledgeNote, NoteKind, RelationKind,
    Relations, SourceStatus, TrustLevel, WikiLink,
};
pub use retrieval::{
    KnowledgeBrief, KnowledgeContextRequest, KnowledgeRouter, KnowledgeSummary, looks_like_command,
    search_queries,
};
pub use vault::{
    FsVaultRepository, VaultError, VaultRepository, VaultWatcher, WatchMode, parse_wikilinks,
};
