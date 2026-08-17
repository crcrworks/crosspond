mod fts;
mod graph;
mod sqlite;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::model::{KnowledgeNote, NewKnowledgeNote};
use crate::vault::{FsVaultRepository, VaultError, VaultRepository, VaultWatcher, WatchMode};

pub use fts::SearchHit;
pub use graph::IndexedLink;
pub use sqlite::{IndexSnapshot, SnapshotNote};

const SCHEMA_VERSION: i32 = 1;

pub struct SearchIndex {
    path: PathBuf,
    conn: Mutex<Connection>,
}

pub struct IndexedVault {
    repo: FsVaultRepository,
    index: Arc<SearchIndex>,
}

pub fn vault_index_id(vault_root: &Path) -> String {
    let canonical = vault_root
        .canonicalize()
        .unwrap_or_else(|_| vault_root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let mut out = String::new();
    for byte in digest.iter().take(16) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn index_db_path(cache_root: &Path, vault_root: &Path) -> PathBuf {
    cache_root
        .join("index")
        .join(format!("{}.sqlite", vault_index_id(vault_root)))
}

pub fn index_note_id(note: &KnowledgeNote) -> String {
    match &note.id {
        Some(id) => id.to_string(),
        None => format!("path:{}", path_str(&note.path)),
    }
}

pub(crate) fn path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl IndexedVault {
    pub fn open(
        vault_root: impl Into<PathBuf>,
        index_path: impl Into<PathBuf>,
    ) -> Result<Self, VaultError> {
        let repo = FsVaultRepository::open(vault_root)?;
        let index = Arc::new(SearchIndex::open(index_path)?);
        index.rebuild(&repo.list_notes()?)?;
        Ok(Self { repo, index })
    }

    pub fn repository(&self) -> &FsVaultRepository {
        &self.repo
    }

    pub fn index(&self) -> &SearchIndex {
        &self.index
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, VaultError> {
        self.index.search(query, limit)
    }

    pub fn rebuild(&self) -> Result<(), VaultError> {
        self.index.rebuild(&self.repo.list_notes()?)
    }

    pub fn create_note(&self, note: NewKnowledgeNote) -> Result<KnowledgeNote, VaultError> {
        let written = self.repo.create_note(note)?;
        self.index.upsert_note(&written)?;
        Ok(written)
    }

    pub fn apply_patch(
        &self,
        patch: crate::model::KnowledgePatch,
    ) -> Result<KnowledgeNote, VaultError> {
        let written = self.repo.apply_patch(patch)?;
        self.index.upsert_note(&written)?;
        Ok(written)
    }

    pub fn read_indexed(&self, id: &str) -> Result<KnowledgeNote, VaultError> {
        if let Some(relative) = id.strip_prefix("path:") {
            let absolute = self.repo.root().join(relative);
            let bytes = std::fs::read(&absolute).map_err(|err| VaultError::Io(err.to_string()))?;
            return crate::vault::parse_markdown(self.repo.root(), &absolute, &bytes);
        }
        let knowledge_id = id
            .parse::<crate::model::KnowledgeId>()
            .map_err(|_| VaultError::InvalidId)?;
        self.repo.read_note(&knowledge_id)
    }

    pub fn find_procedure(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, VaultError> {
        let hits = self.search(query, limit.max(20))?;
        Ok(hits
            .into_iter()
            .filter(|hit| hit.kind == crate::model::NoteKind::Procedure)
            .take(limit)
            .collect())
    }

    pub fn watch(&self, debounce: Duration, mode: WatchMode) -> Result<VaultWatcher, VaultError> {
        VaultWatcher::start(
            self.repo.root().to_path_buf(),
            Arc::clone(&self.index),
            debounce,
            mode,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NewKnowledgeNote, NoteKind, Relations, TrustLevel};
    use crate::vault::{FsVaultRepository, VaultRepository, format_wikilink};
    use std::fs;
    use std::str::FromStr;

    fn temp_paths() -> (PathBuf, PathBuf) {
        let id = uuid::Uuid::now_v7();
        let vault = std::env::temp_dir().join(format!("crosspond-index-vault-{id}"));
        let sqlite = std::env::temp_dir().join(format!("crosspond-index-db-{id}.sqlite"));
        (vault, sqlite)
    }

    fn resource(title: &str, aliases: &[&str], body: &str) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind: NoteKind::Resource,
            title: title.into(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            tags: vec!["lab".into()],
            trust: TrustLevel::User,
            relations: Relations::default(),
            resource_kind: Some("vpn".into()),
            body: body.into(),
            relative_path: None,
            url: None,
            source_kind: None,
            source_status: None,
        }
    }

    #[test]
    fn vault_index_id_is_stable_for_the_same_path() {
        let root = std::env::temp_dir().join(format!("crosspond-id-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&root).unwrap();
        let first = vault_index_id(&root);
        let second = vault_index_id(&root);
        assert_eq!(first, second);
        assert_eq!(first.len(), 32);
        let other = std::env::temp_dir().join(format!("crosspond-id-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&other).unwrap();
        assert_ne!(vault_index_id(&root), vault_index_id(&other));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn search_finds_title_alias_and_body() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        indexed
            .create_note(resource(
                "Lab VPN",
                &["研究室VPN"],
                "# Lab VPN\n\nNeeds the lab profile.\n",
            ))
            .unwrap();
        let by_title = indexed.search("VPN", 10).unwrap();
        assert!(by_title.iter().any(|hit| hit.title == "Lab VPN"));
        let by_alias = indexed.search("研究室VPN", 10).unwrap();
        assert!(by_alias.iter().any(|hit| hit.title == "Lab VPN"));
        let by_body = indexed.search("lab profile", 10).unwrap();
        assert!(by_body.iter().any(|hit| hit.title == "Lab VPN"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn filename_wikilinks_resolve_when_title_is_obsidian_illegal() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let paper = indexed
            .create_note(resource("cordiverse/paper: Title", &[], "# Paper\n"))
            .unwrap();
        let link = format_wikilink(&paper.title, &paper.path);
        indexed
            .create_note(resource(
                "Related Note",
                &[],
                &format!("# Related\n\nSee {link}.\n"),
            ))
            .unwrap();
        let snapshot = indexed.index.snapshot().unwrap();
        let paper_id = paper.id.unwrap().to_string();
        assert!(
            snapshot
                .links
                .iter()
                .any(|edge| { edge.relation_type == "wikilink" && edge.target_id == paper_id })
        );
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn rebuild_after_deleting_sqlite_matches_previous_snapshot() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let mut wiki = resource("Lab Wiki", &[], "# Lab Wiki\n\nInternal docs.\n");
        wiki.relative_path = Some(PathBuf::from("resources/Lab Wiki.md"));
        indexed.create_note(wiki).unwrap();
        let mut vpn = resource(
            "Lab VPN",
            &["研究室VPN"],
            "# Lab VPN\n\nNeeds [[Lab Wiki]].\n",
        );
        vpn.relations.uses =
            vec![crate::model::KnowledgeId::from_str("cp_resource_lab_wiki").unwrap()];
        indexed.create_note(vpn).unwrap();
        let before = indexed.index.snapshot().unwrap();
        assert!(
            before
                .links
                .iter()
                .any(|link| link.relation_type == "wikilink")
        );
        assert!(before.links.iter().any(|link| link.relation_type == "uses"));
        drop(indexed);
        fs::remove_file(&sqlite).unwrap();
        let rebuilt = IndexedVault::open(&vault, &sqlite).unwrap();
        let after = rebuilt.index.snapshot().unwrap();
        assert_eq!(before, after);
        let hits = rebuilt.search("Lab Wiki", 10).unwrap();
        assert!(hits.iter().any(|hit| hit.title == "Lab Wiki"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn find_procedure_filters_to_procedure_notes() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        indexed
            .create_note(resource(
                "Lab VPN",
                &["研究室VPN"],
                "# Lab VPN\n\nNeeds the lab profile.\n",
            ))
            .unwrap();
        indexed
            .create_note(NewKnowledgeNote {
                kind: NoteKind::Procedure,
                title: "Check Lab Assignment".into(),
                aliases: vec!["研究室の課題確認".into()],
                tags: vec!["lab".into()],
                trust: TrustLevel::User,
                relations: Relations::default(),
                resource_kind: None,
                body: "# Check Lab Assignment\n\nHow to retrieve assignments.\n".into(),
                relative_path: None,
                url: None,
                source_kind: None,
                source_status: None,
            })
            .unwrap();
        let hits = indexed.find_procedure("研究室の課題確認", 8).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Check Lab Assignment");
        assert_eq!(hits[0].kind, NoteKind::Procedure);
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn renamed_note_keeps_its_id_in_the_index() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let created = indexed
            .create_note(resource("Lab VPN", &[], "# Lab VPN\n"))
            .unwrap();
        let id = created.id.clone().unwrap();
        fs::rename(
            vault.join("resources/Lab VPN.md"),
            vault.join("resources/Laboratory VPN.md"),
        )
        .unwrap();
        indexed.rebuild().unwrap();
        let hits = indexed.search("Lab VPN", 10).unwrap();
        assert_eq!(hits[0].id, id.to_string());
        assert_eq!(hits[0].path, PathBuf::from("resources/Laboratory VPN.md"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn unmanaged_manual_note_is_searchable() {
        let (vault, sqlite) = temp_paths();
        let repo = FsVaultRepository::open(&vault).unwrap();
        fs::write(
            vault.join("resources/Lab File Server.md"),
            "# Lab File Server\n\nsmb://lab-files\n",
        )
        .unwrap();
        let index = SearchIndex::open(&sqlite).unwrap();
        index.rebuild(&repo.list_notes().unwrap()).unwrap();
        let hits = index.search("File Server", 10).unwrap();
        assert!(hits.iter().any(|hit| hit.title == "Lab File Server"));
        assert!(hits.iter().any(|hit| hit.id.starts_with("path:")));
        let reserved = index.search("Knowledge Index", 10).unwrap();
        assert!(reserved.is_empty());
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn upsert_updates_searchable_body() {
        let (vault, sqlite) = temp_paths();
        let repo = FsVaultRepository::open(&vault).unwrap();
        let path = vault.join("resources/Lab Wiki.md");
        fs::write(&path, "# Lab Wiki\n\nManual Obsidian note.\n").unwrap();
        let index = SearchIndex::open(&sqlite).unwrap();
        index.rebuild(&repo.list_notes().unwrap()).unwrap();
        assert!(
            index
                .search("Obsidian", 10)
                .unwrap()
                .iter()
                .any(|hit| hit.title == "Lab Wiki")
        );
        fs::write(&path, "# Lab Wiki\n\nUpdated assignment page.\n").unwrap();
        let bytes = fs::read(&path).unwrap();
        let note = crate::vault::parse_markdown(repo.root(), &path, &bytes).unwrap();
        index.upsert_note(&note).unwrap();
        assert!(
            index
                .search("assignment", 10)
                .unwrap()
                .iter()
                .any(|hit| hit.title == "Lab Wiki")
        );
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }
}
