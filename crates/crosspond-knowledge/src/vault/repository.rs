use std::fs;
use std::path::{Path, PathBuf};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::VaultError;
use super::hash::content_hash;
use super::index::{append_log, rebuild_index};
use super::parser::parse_markdown;
use super::paths::{
    default_relative_path, format_wikilink, is_indexable_markdown, is_inside, is_reserved_relative,
    join_inside, sanitize_relative_path,
};
use super::schema::{HOME_MD, SCHEMA_MD};
use super::writer::render_note;
use crate::model::{KnowledgeId, KnowledgeNote, KnowledgePatch, NewKnowledgeNote};

pub trait VaultRepository: Send + Sync {
    fn root(&self) -> &Path;
    fn read_note(&self, id: &KnowledgeId) -> Result<KnowledgeNote, VaultError>;
    fn create_note(&self, note: NewKnowledgeNote) -> Result<KnowledgeNote, VaultError>;
    fn apply_patch(&self, patch: KnowledgePatch) -> Result<KnowledgeNote, VaultError>;
}

pub struct FsVaultRepository {
    root: PathBuf,
}

impl FsVaultRepository {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, VaultError> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(VaultError::EmptyRoot);
        }
        fs::create_dir_all(&root).map_err(|err| VaultError::Io(err.to_string()))?;
        let repo = Self {
            root: root
                .canonicalize()
                .map_err(|err| VaultError::Io(err.to_string()))?,
        };
        repo.ensure_layout()?;
        Ok(repo)
    }

    pub fn list_notes(&self) -> Result<Vec<KnowledgeNote>, VaultError> {
        self.scan_notes()
    }

    fn ensure_layout(&self) -> Result<(), VaultError> {
        for dir in [
            "knowledge/concepts",
            "knowledge/entities",
            "knowledge/syntheses",
            "procedures",
            "resources",
            "sources/assets",
            "history",
            "_system",
        ] {
            fs::create_dir_all(self.root.join(dir))
                .map_err(|err| VaultError::Io(err.to_string()))?;
        }
        write_if_missing(&self.root.join("_system/Schema.md"), SCHEMA_MD)?;
        write_if_missing(&self.root.join("Home.md"), HOME_MD)?;
        write_if_missing(&self.root.join("Log.md"), "# Knowledge Log\n")?;
        let notes = self.scan_notes()?;
        rebuild_index(&self.root, &notes)?;
        Ok(())
    }

    fn scan_notes(&self) -> Result<Vec<KnowledgeNote>, VaultError> {
        let mut notes = Vec::new();
        let mut files = Vec::new();
        collect_markdown(&self.root, &self.root, &mut files)?;
        for path in files {
            let bytes = fs::read(&path).map_err(|err| VaultError::Io(err.to_string()))?;
            notes.push(parse_markdown(&self.root, &path, &bytes)?);
        }
        Ok(notes)
    }

    fn find_by_id(&self, id: &KnowledgeId) -> Result<KnowledgeNote, VaultError> {
        let mut found = None;
        for note in self.scan_notes()? {
            if note.id.as_ref() == Some(id) {
                if found.is_some() {
                    return Err(VaultError::DuplicateId(id.to_string()));
                }
                found = Some(note);
            }
        }
        found.ok_or_else(|| VaultError::NotFound(id.to_string()))
    }

    fn write_note(&self, note: &KnowledgeNote) -> Result<KnowledgeNote, VaultError> {
        let absolute = join_inside(&self.root, &note.path)?;
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).map_err(|err| VaultError::Io(err.to_string()))?;
        }
        let rendered = render_note(note);
        fs::write(&absolute, rendered.as_bytes()).map_err(|err| VaultError::Io(err.to_string()))?;
        if !is_inside(&self.root, &absolute) {
            let _ = fs::remove_file(&absolute);
            return Err(VaultError::PathEscape);
        }
        let bytes = fs::read(&absolute).map_err(|err| VaultError::Io(err.to_string()))?;
        parse_markdown(&self.root, &absolute, &bytes)
    }

    fn refresh_navigation(&self, heading: &str, log_lines: &[String]) -> Result<(), VaultError> {
        let notes = self.scan_notes()?;
        rebuild_index(&self.root, &notes)?;
        append_log(&self.root, heading, log_lines)
    }
}

impl VaultRepository for FsVaultRepository {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read_note(&self, id: &KnowledgeId) -> Result<KnowledgeNote, VaultError> {
        self.find_by_id(id)
    }

    fn create_note(&self, new: NewKnowledgeNote) -> Result<KnowledgeNote, VaultError> {
        let now = OffsetDateTime::now_utc();
        let stamp = now
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
        let relative = match new.relative_path {
            Some(path) => sanitize_relative_path(&path)?,
            None => default_relative_path(new.kind, &new.title, &now)?,
        };
        if is_reserved_relative(&relative) {
            return Err(VaultError::ReservedPath(relative.display().to_string()));
        }
        let absolute = join_inside(&self.root, &relative)?;
        if absolute.exists() {
            return Err(VaultError::DuplicatePath(relative.display().to_string()));
        }
        let id = KnowledgeId::generate();
        if self
            .scan_notes()?
            .iter()
            .any(|note| note.id.as_ref() == Some(&id))
        {
            return Err(VaultError::DuplicateId(id.to_string()));
        }
        let note = KnowledgeNote {
            id: Some(id),
            kind: new.kind,
            title: new.title.clone(),
            aliases: new.aliases,
            tags: new.tags,
            trust: Some(new.trust),
            relations: new.relations,
            sources: Vec::new(),
            resource_kind: new.resource_kind,
            credential_ref: new.credential_ref,
            last_verified: None,
            source_kind: new.source_kind,
            url: new.url,
            source_status: new.source_status,
            created: Some(stamp.clone()),
            updated: Some(stamp.clone()),
            body: new.body,
            path: relative.clone(),
            content_hash: String::new(),
        };
        let written = self.write_note(&note)?;
        let day = stamp.get(..10).unwrap_or(&stamp);
        self.refresh_navigation(
            &format!("{day} — Note created"),
            &[
                "Created:".into(),
                format!("- {}", format_wikilink(&written.title, &written.path)),
            ],
        )?;
        Ok(written)
    }

    fn apply_patch(&self, patch: KnowledgePatch) -> Result<KnowledgeNote, VaultError> {
        let mut note = self.find_by_id(&patch.id)?;
        let absolute = join_inside(&self.root, &note.path)?;
        let bytes = fs::read(&absolute).map_err(|err| VaultError::Io(err.to_string()))?;
        let current_hash = content_hash(&bytes);
        if current_hash != patch.expected_hash {
            return Err(VaultError::Conflict(patch.id.to_string()));
        }
        if let Some(title) = patch.title {
            note.title = title;
        }
        if let Some(aliases) = patch.aliases {
            note.aliases = aliases;
        }
        if let Some(tags) = patch.tags {
            note.tags = tags;
        }
        if let Some(trust) = patch.trust {
            note.trust = Some(trust);
        }
        if let Some(relations) = patch.relations {
            note.relations = relations;
        }
        if let Some(last_verified) = patch.last_verified {
            note.last_verified = Some(last_verified);
        }
        if let Some(status) = patch.source_status {
            note.source_status = Some(status);
        }
        if let Some(body) = patch.body {
            note.body = body;
        }
        note.updated = Some(
            OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into()),
        );
        let written = self.write_note(&note)?;
        let day = written
            .updated
            .as_deref()
            .and_then(|stamp| stamp.get(..10))
            .unwrap_or("unknown");
        self.refresh_navigation(
            &format!("{day} — Note updated"),
            &[
                "Updated:".into(),
                format!("- {}", format_wikilink(&written.title, &written.path)),
            ],
        )?;
        Ok(written)
    }
}

fn write_if_missing(path: &Path, contents: &str) -> Result<(), VaultError> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| VaultError::Io(err.to_string()))?;
    }
    fs::write(path, contents).map_err(|err| VaultError::Io(err.to_string()))
}

fn collect_markdown(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), VaultError> {
    let entries = fs::read_dir(dir).map_err(|err| VaultError::Io(err.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|err| VaultError::Io(err.to_string()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| VaultError::Io(err.to_string()))?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "_system" {
                continue;
            }
            collect_markdown(root, &path, out)?;
        } else if is_indexable_markdown(root, &path) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NoteKind, Relations, TrustLevel};
    use std::os::unix::fs::symlink;
    use std::str::FromStr;

    fn temp_vault() -> (FsVaultRepository, PathBuf) {
        let root = std::env::temp_dir().join(format!("crosspond-vault-{}", uuid::Uuid::now_v7()));
        let repo = FsVaultRepository::open(&root).unwrap();
        (repo, root)
    }

    fn resource(title: &str, aliases: &[&str]) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind: NoteKind::Resource,
            title: title.into(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            tags: vec!["lab".into()],
            trust: TrustLevel::User,
            relations: Relations::default(),
            resource_kind: Some("vpn".into()),
            credential_ref: None,
            body: "# Lab VPN\n\nVPN required to access internal laboratory services.\n".into(),
            relative_path: None,
            url: None,
            source_kind: None,
            source_status: None,
        }
    }

    #[test]
    fn creates_obsidian_resource_note() {
        let (repo, root) = temp_vault();
        let note = repo
            .create_note(resource("Lab VPN", &["研究室VPN"]))
            .unwrap();
        let path = root.join("resources/Lab VPN.md");
        assert!(path.is_file());
        assert_eq!(note.path, PathBuf::from("resources/Lab VPN.md"));
        assert_eq!(note.kind, NoteKind::Resource);
        assert!(note.id.unwrap().as_str().starts_with("cp_"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("---\n"));
        assert!(text.contains("type: resource"));
        assert!(text.contains("title: Lab VPN"));
        assert!(text.contains("研究室VPN"));
        assert!(!text.contains("password"));
        assert!(!text.contains("api_key"));
        let index = fs::read_to_string(root.join("Index.md")).unwrap();
        assert!(index.contains("[[Lab VPN]]"));
        let log = fs::read_to_string(root.join("Log.md")).unwrap();
        assert!(log.contains("[[Lab VPN]]"));
        assert!(root.join("_system/Schema.md").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn japanese_title_becomes_unicode_filename() {
        let (repo, root) = temp_vault();
        let note = repo
            .create_note(resource("研究室VPN", &["Lab VPN"]))
            .unwrap();
        assert_eq!(note.path, PathBuf::from("resources/研究室VPN.md"));
        assert!(root.join("resources/研究室VPN.md").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn titles_with_obsidian_illegal_chars_use_filename_wikilinks() {
        let (repo, root) = temp_vault();
        let title = "cordiverse/paper: A Programming Paradigm";
        let note = repo.create_note(resource(title, &[])).unwrap();
        assert_eq!(
            note.path,
            PathBuf::from("resources/cordiverse-paper- A Programming Paradigm.md")
        );
        assert!(root.join(&note.path).is_file());
        let expected = format_wikilink(title, &note.path);
        assert_eq!(
            expected,
            "[[cordiverse-paper- A Programming Paradigm|cordiverse/paper: A Programming Paradigm]]"
        );
        let index = fs::read_to_string(root.join("Index.md")).unwrap();
        assert!(index.contains(&expected));
        assert!(!index.contains("[[cordiverse/paper: A Programming Paradigm]]"));
        let log = fs::read_to_string(root.join("Log.md")).unwrap();
        assert!(log.contains(&expected));
        fs::write(
            root.join("Index.md"),
            "- [[cordiverse/paper: A Programming Paradigm]]\n",
        )
        .unwrap();
        drop(repo);
        let _repo = FsVaultRepository::open(&root).unwrap();
        let rewritten = fs::read_to_string(root.join("Index.md")).unwrap();
        assert!(rewritten.contains(&expected));
        assert!(!rewritten.contains("[[cordiverse/paper: A Programming Paradigm]]"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn custom_relative_path_sanitizes_illegal_filename_chars() {
        let (repo, root) = temp_vault();
        let mut new = resource("Safe Title", &[]);
        new.relative_path = Some(PathBuf::from("resources/cordiverse/paper: Title.md"));
        let note = repo.create_note(new).unwrap();
        assert_eq!(
            note.path,
            PathBuf::from("resources/cordiverse/paper- Title.md")
        );
        assert!(root.join(&note.path).is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn renamed_file_is_still_found_by_id() {
        let (repo, root) = temp_vault();
        let created = repo.create_note(resource("Lab VPN", &[])).unwrap();
        let id = created.id.clone().unwrap();
        fs::rename(
            root.join("resources/Lab VPN.md"),
            root.join("resources/Laboratory VPN.md"),
        )
        .unwrap();
        let read = repo.read_note(&id).unwrap();
        assert_eq!(read.title, "Lab VPN");
        assert_eq!(read.path, PathBuf::from("resources/Laboratory VPN.md"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_ids_are_rejected_on_read() {
        let (repo, root) = temp_vault();
        let created = repo.create_note(resource("Lab VPN", &[])).unwrap();
        let id = created.id.unwrap();
        let mut copy = fs::read_to_string(root.join("resources/Lab VPN.md")).unwrap();
        copy = copy.replace("title: Lab VPN", "title: Copy");
        fs::write(root.join("resources/Copy.md"), copy).unwrap();
        let err = repo.read_note(&id).unwrap_err();
        assert_eq!(err, VaultError::DuplicateId(id.to_string()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_aborts_when_file_changed_externally() {
        let (repo, root) = temp_vault();
        let created = repo.create_note(resource("Lab VPN", &[])).unwrap();
        let path = root.join("resources/Lab VPN.md");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("\nEdited in Obsidian.\n");
        fs::write(&path, text).unwrap();
        let err = repo
            .apply_patch(KnowledgePatch {
                id: created.id.unwrap(),
                expected_hash: created.content_hash,
                title: Some("Laboratory VPN".into()),
                aliases: None,
                tags: None,
                trust: None,
                relations: None,
                last_verified: None,
                source_status: None,
                body: None,
            })
            .unwrap_err();
        assert!(matches!(err, VaultError::Conflict(_)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parent_escape_is_rejected() {
        let (repo, root) = temp_vault();
        let err = repo
            .create_note(NewKnowledgeNote {
                relative_path: Some(PathBuf::from("../escape.md")),
                ..resource("Escape", &[])
            })
            .unwrap_err();
        assert_eq!(err, VaultError::PathEscape);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn symlink_escape_is_rejected() {
        let (repo, root) = temp_vault();
        let outside =
            std::env::temp_dir().join(format!("crosspond-vault-out-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("resources/link")).unwrap();
        let err = repo
            .create_note(NewKnowledgeNote {
                relative_path: Some(PathBuf::from("resources/link/secret.md")),
                ..resource("Secret", &[])
            })
            .unwrap_err();
        assert_eq!(err, VaultError::PathEscape);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn successful_patch_updates_body() {
        let (repo, root) = temp_vault();
        let created = repo.create_note(resource("Lab VPN", &[])).unwrap();
        let updated = repo
            .apply_patch(KnowledgePatch {
                id: created.id.clone().unwrap(),
                expected_hash: created.content_hash,
                title: None,
                aliases: None,
                tags: None,
                trust: None,
                relations: None,
                last_verified: None,
                source_status: None,
                body: Some("# Lab VPN\n\nUpdated profile: Lab.\n".into()),
            })
            .unwrap();
        assert!(updated.body.contains("Updated profile"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn knowledge_id_from_str_rejects_empty() {
        assert!(KnowledgeId::from_str("  ").is_err());
        assert!(KnowledgeId::from_str("cp_ok").is_ok());
    }

    #[test]
    fn create_note_roundtrips_uses_relation() {
        let (repo, root) = temp_vault();
        let mut new = resource("Lab VPN", &[]);
        new.relations.uses = vec![KnowledgeId::from_str("cp_resource_lab_wiki").unwrap()];
        let note = repo.create_note(new).unwrap();
        assert_eq!(note.relations.uses[0].as_str(), "cp_resource_lab_wiki");
        let text = fs::read_to_string(root.join("resources/Lab VPN.md")).unwrap();
        assert!(text.contains("uses:"));
        assert!(text.contains("cp_resource_lab_wiki"));
        let _ = fs::remove_dir_all(root);
    }
}
