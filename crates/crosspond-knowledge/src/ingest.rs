use std::collections::HashSet;
use std::str::FromStr;

use crate::index::IndexedVault;
use crate::model::{
    KnowledgeId, KnowledgeNote, KnowledgePatch, NewKnowledgeNote, NoteKind, RelationKind,
    Relations, SourceStatus, TrustLevel,
};
use crate::retrieval::search_queries;
use crate::vault::{VaultError, content_hash, format_wikilink};

const MAX_CANDIDATES: usize = 8;
const MAX_CREATE: usize = 2;
const SECRET_MARKERS: &[&str] = &[
    "api_key",
    "secret_key",
    "password=",
    "password:",
    "-----begin ",
    "credential_ref:",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCapture {
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    pub source_kind: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub id: KnowledgeId,
    pub title: String,
    pub kind: NoteKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateNoteProposal {
    pub kind: NoteKind,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateNoteProposal {
    pub id: KnowledgeId,
    pub title: String,
    pub expected_hash: String,
    pub append: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkProposal {
    pub from: KnowledgeId,
    pub to: KnowledgeId,
    pub relation: RelationKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeConflict {
    pub note_id: Option<KnowledgeId>,
    pub title: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngestionPlan {
    pub source_id: KnowledgeId,
    pub source_title: String,
    pub duplicate: bool,
    pub create_notes: Vec<CreateNoteProposal>,
    pub update_notes: Vec<UpdateNoteProposal>,
    pub links: Vec<LinkProposal>,
    pub conflicts: Vec<KnowledgeConflict>,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IngestionOutcome {
    pub source_id: KnowledgeId,
    pub created: Vec<KnowledgeId>,
    pub updated: Vec<KnowledgeId>,
    pub conflicts: Vec<KnowledgeConflict>,
}

pub struct IngestionEngine<'a> {
    vault: &'a IndexedVault,
}

impl<'a> IngestionEngine<'a> {
    pub fn new(vault: &'a IndexedVault) -> Self {
        Self { vault }
    }

    pub fn ingest(&self, capture: SourceCapture) -> Result<IngestionPlan, VaultError> {
        let capture = validate_capture(capture)?;
        let fingerprint = source_fingerprint(&capture);
        if let Some(existing) = self.find_duplicate(&fingerprint)? {
            let mut plan = self.plan_for_source(&existing, &capture.body)?;
            plan.duplicate = true;
            return Ok(plan);
        }
        let source = self.vault.create_note(NewKnowledgeNote {
            kind: NoteKind::Source,
            title: capture.title.clone(),
            aliases: Vec::new(),
            tags: Vec::new(),
            trust: TrustLevel::External,
            relations: Relations::default(),
            resource_kind: None,
            body: source_body(&capture, &fingerprint),
            relative_path: None,
            url: capture.url.clone(),
            source_kind: capture.source_kind.clone(),
            source_status: Some(SourceStatus::Processed),
        })?;
        let source_id = source.id.clone().ok_or(VaultError::InvalidId)?;
        self.plan_for_source(&source, &capture.body)
            .map(|mut plan| {
                plan.source_id = source_id;
                plan
            })
    }

    pub fn save_unread(&self, capture: SourceCapture) -> Result<KnowledgeNote, VaultError> {
        let capture = validate_capture(capture)?;
        let fingerprint = source_fingerprint(&capture);
        if let Some(existing) = self.find_duplicate(&fingerprint)? {
            return Ok(existing);
        }
        self.vault.create_note(NewKnowledgeNote {
            kind: NoteKind::Source,
            title: capture.title.clone(),
            aliases: Vec::new(),
            tags: Vec::new(),
            trust: TrustLevel::External,
            relations: Relations::default(),
            resource_kind: None,
            body: source_body(&capture, &fingerprint),
            relative_path: None,
            url: capture.url.clone(),
            source_kind: capture.source_kind.clone(),
            source_status: Some(SourceStatus::Unread),
        })
    }

    pub fn set_status(
        &self,
        source_id: &str,
        status: SourceStatus,
    ) -> Result<KnowledgeNote, VaultError> {
        let note = self.vault.read_indexed(source_id)?;
        if note.kind != NoteKind::Source {
            return Err(VaultError::Io("id is not a source note".into()));
        }
        let Some(id) = note.id.clone() else {
            return Err(VaultError::InvalidId);
        };
        if note.source_status == Some(status) {
            return Ok(note);
        }
        self.vault.apply_patch(KnowledgePatch {
            id,
            expected_hash: note.content_hash,
            title: None,
            aliases: None,
            tags: None,
            trust: None,
            relations: None,
            last_verified: None,
            source_status: Some(status),
            body: None,
        })
    }

    pub fn propose(&self, source_id: &str) -> Result<IngestionPlan, VaultError> {
        let note = self.vault.read_indexed(source_id)?;
        if note.kind != NoteKind::Source {
            return Err(VaultError::Io("id is not a source note".into()));
        }
        self.plan_for_source(&note, &note.body)
    }

    pub fn apply(&self, plan: &IngestionPlan) -> Result<IngestionOutcome, VaultError> {
        let mut outcome = IngestionOutcome {
            source_id: plan.source_id.clone(),
            created: Vec::new(),
            updated: Vec::new(),
            conflicts: plan.conflicts.clone(),
        };
        if plan.duplicate {
            return Ok(outcome);
        }
        let allowed: HashSet<String> = plan
            .candidates
            .iter()
            .map(|candidate| candidate.id.to_string())
            .chain(std::iter::once(plan.source_id.to_string()))
            .collect();
        for create in &plan.create_notes {
            match self.vault.create_note(NewKnowledgeNote {
                kind: create.kind,
                title: create.title.clone(),
                aliases: Vec::new(),
                tags: Vec::new(),
                trust: TrustLevel::Derived,
                relations: {
                    let mut relations = Relations::default();
                    relations.derived_from.push(plan.source_id.clone());
                    relations
                },
                resource_kind: None,
                body: create.body.clone(),
                relative_path: None,
                url: None,
                source_kind: None,
                source_status: None,
            }) {
                Ok(note) => {
                    if let Some(id) = note.id {
                        let _ = self.apply_link(&LinkProposal {
                            from: plan.source_id.clone(),
                            to: id.clone(),
                            relation: RelationKind::Mentions,
                        });
                        outcome.created.push(id);
                    }
                }
                Err(VaultError::DuplicatePath(path)) => outcome.conflicts.push(KnowledgeConflict {
                    note_id: None,
                    title: create.title.clone(),
                    reason: format!("already exists at {path}"),
                }),
                Err(err) => return Err(err),
            }
        }
        for update in &plan.update_notes {
            if !allowed.contains(update.id.as_str()) {
                outcome.conflicts.push(KnowledgeConflict {
                    note_id: Some(update.id.clone()),
                    title: update.title.clone(),
                    reason: "update target was not a retrieved candidate".into(),
                });
                continue;
            }
            match self.apply_update(plan, update) {
                Ok(()) => outcome.updated.push(update.id.clone()),
                Err(VaultError::Conflict(id)) => outcome.conflicts.push(KnowledgeConflict {
                    note_id: Some(update.id.clone()),
                    title: update.title.clone(),
                    reason: format!("note `{id}` was modified externally"),
                }),
                Err(VaultError::NotFound(id)) => outcome.conflicts.push(KnowledgeConflict {
                    note_id: Some(update.id.clone()),
                    title: update.title.clone(),
                    reason: format!("note `{id}` is no longer in the vault"),
                }),
                Err(err) => return Err(err),
            }
        }
        for link in &plan.links {
            if !allowed.contains(link.from.as_str()) || !allowed.contains(link.to.as_str()) {
                outcome.conflicts.push(KnowledgeConflict {
                    note_id: Some(link.from.clone()),
                    title: plan.source_title.clone(),
                    reason: "link target was not a retrieved candidate".into(),
                });
                continue;
            }
            match self.apply_link(link) {
                Ok(()) => {}
                Err(VaultError::Conflict(id)) => outcome.conflicts.push(KnowledgeConflict {
                    note_id: Some(link.from.clone()),
                    title: plan.source_title.clone(),
                    reason: format!("note `{id}` was modified externally"),
                }),
                Err(VaultError::NotFound(id)) => outcome.conflicts.push(KnowledgeConflict {
                    note_id: Some(link.from.clone()),
                    title: plan.source_title.clone(),
                    reason: format!("note `{id}` is no longer in the vault"),
                }),
                Err(err) => return Err(err),
            }
        }
        Ok(outcome)
    }

    fn find_duplicate(&self, fingerprint: &str) -> Result<Option<KnowledgeNote>, VaultError> {
        for hit in self.vault.search(fingerprint, 8)? {
            let Ok(note) = self.vault.read_indexed(&hit.id) else {
                continue;
            };
            if note.kind == NoteKind::Source && note.body.contains(fingerprint) {
                return Ok(Some(note));
            }
        }
        Ok(None)
    }

    fn plan_for_source(
        &self,
        source: &KnowledgeNote,
        original_body: &str,
    ) -> Result<IngestionPlan, VaultError> {
        let source_id = source.id.clone().ok_or(VaultError::InvalidId)?;
        let candidates = self.candidates(&source.title, original_body, &source_id)?;
        let mut plan = IngestionPlan {
            source_id: source_id.clone(),
            source_title: source.title.clone(),
            duplicate: false,
            create_notes: Vec::new(),
            update_notes: Vec::new(),
            links: Vec::new(),
            conflicts: Vec::new(),
            candidates: candidates.clone(),
        };
        let haystack = format!("{}\n{}", source.title, original_body).to_lowercase();
        let mut mentioned_knowledge = false;
        for candidate in &candidates {
            if !mentioned(&haystack, candidate) {
                continue;
            }
            plan.links.push(LinkProposal {
                from: source_id.clone(),
                to: candidate.id.clone(),
                relation: RelationKind::Mentions,
            });
            if candidate.kind == NoteKind::Knowledge
                || candidate.kind == NoteKind::Synthesis
                || candidate.kind == NoteKind::Procedure
            {
                if candidate.kind != NoteKind::Procedure {
                    mentioned_knowledge = true;
                }
                let Ok(note) = self.vault.read_indexed(candidate.id.as_str()) else {
                    continue;
                };
                plan.update_notes.push(UpdateNoteProposal {
                    id: candidate.id.clone(),
                    title: candidate.title.clone(),
                    expected_hash: note.content_hash,
                    append: provenance_append(source),
                });
            }
        }
        if !mentioned_knowledge && plan.create_notes.len() < MAX_CREATE {
            let title = knowledge_title_from_source(&source.title);
            if !candidates.iter().any(|candidate| candidate.title == title) {
                plan.create_notes.push(CreateNoteProposal {
                    kind: NoteKind::Knowledge,
                    title: title.clone(),
                    body: format!(
                        "# {title}\n\nDerived from {}.\n\n{}\n",
                        format_wikilink(&source.title, &source.path),
                        first_paragraph(original_body)
                    ),
                });
            }
        }
        Ok(plan)
    }

    fn candidates(
        &self,
        title: &str,
        body: &str,
        source_id: &KnowledgeId,
    ) -> Result<Vec<Candidate>, VaultError> {
        let mut queries = search_queries(title);
        for token in notable_phrases(body) {
            queries.push(token);
        }
        let mut seen = HashSet::from([source_id.to_string()]);
        let mut out = Vec::new();
        for query in queries {
            for hit in self.vault.search(&query, 8)? {
                if !seen.insert(hit.id.clone()) {
                    continue;
                }
                if matches!(hit.kind, NoteKind::Source | NoteKind::Activity) {
                    continue;
                }
                let Ok(id) = KnowledgeId::from_str(&hit.id) else {
                    continue;
                };
                out.push(Candidate {
                    id,
                    title: hit.title,
                    kind: hit.kind,
                });
                if out.len() >= MAX_CANDIDATES {
                    return Ok(out);
                }
            }
        }
        Ok(out)
    }

    fn apply_update(
        &self,
        plan: &IngestionPlan,
        update: &UpdateNoteProposal,
    ) -> Result<(), VaultError> {
        let note = self.vault.read_indexed(update.id.as_str())?;
        let source = self.vault.read_indexed(plan.source_id.as_str())?;
        let link = format_wikilink(&source.title, &source.path);
        if note.body.contains(&link)
            || note.body.contains(&format!("[[{}]]", plan.source_title))
            || note.body.contains(update.append.trim())
        {
            return Ok(());
        }
        let mut relations = note.relations.clone();
        if !relations.derived_from.contains(&plan.source_id) {
            relations.derived_from.push(plan.source_id.clone());
        }
        let mut body = note.body;
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str(&update.append);
        self.vault.apply_patch(KnowledgePatch {
            id: update.id.clone(),
            expected_hash: update.expected_hash.clone(),
            title: None,
            aliases: None,
            tags: None,
            trust: None,
            relations: Some(relations),
            last_verified: None,
            source_status: None,
            body: Some(body),
        })?;
        Ok(())
    }

    fn apply_link(&self, link: &LinkProposal) -> Result<(), VaultError> {
        let note = self.vault.read_indexed(link.from.as_str())?;
        let Some(id) = note.id.clone() else {
            return Ok(());
        };
        let mut relations = note.relations.clone();
        let bucket = relations.ids_for_mut(link.relation);
        if bucket.contains(&link.to) {
            return Ok(());
        }
        bucket.push(link.to.clone());
        self.vault.apply_patch(KnowledgePatch {
            id,
            expected_hash: note.content_hash,
            title: None,
            aliases: None,
            tags: None,
            trust: None,
            relations: Some(relations),
            last_verified: None,
            source_status: None,
            body: None,
        })?;
        Ok(())
    }
}

fn validate_capture(capture: SourceCapture) -> Result<SourceCapture, VaultError> {
    let title = capture.title.trim();
    let body = capture.body.trim();
    if title.is_empty() {
        return Err(VaultError::Io("source title is required".into()));
    }
    if looks_like_secret(title) || looks_like_secret(body) {
        return Err(VaultError::Io(
            "refusing to store a source that looks like a secret".into(),
        ));
    }
    Ok(SourceCapture {
        title: title.to_string(),
        body: body.to_string(),
        url: capture.url.filter(|url| !url.trim().is_empty()),
        source_kind: capture.source_kind.filter(|kind| !kind.trim().is_empty()),
    })
}

pub fn looks_like_secret(text: &str) -> bool {
    let lower = text.to_lowercase();
    SECRET_MARKERS.iter().any(|marker| lower.contains(marker))
}

fn source_fingerprint(capture: &SourceCapture) -> String {
    let mut data = capture.title.clone();
    data.push('\n');
    data.push_str(&capture.body);
    data.push('\n');
    if let Some(url) = &capture.url {
        data.push_str(url);
    }
    content_hash(data.as_bytes())
}

fn source_body(capture: &SourceCapture, fingerprint: &str) -> String {
    let mut body = format!("# {}\n\n{}\n", capture.title, capture.body.trim());
    if let Some(url) = &capture.url {
        body.push_str(&format!("\nURL: {url}\n"));
    }
    body.push_str(&format!("\nFingerprint: {fingerprint}\n"));
    body
}

fn mentioned(haystack: &str, candidate: &Candidate) -> bool {
    let title = candidate.title.to_lowercase();
    !title.is_empty() && haystack.contains(&title)
}

fn notable_phrases(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| line.chars().count() >= 4 && line.chars().count() <= 80)
        .filter(|line| !line.starts_with('#'))
        .take(6)
        .map(str::to_string)
        .collect()
}

fn knowledge_title_from_source(title: &str) -> String {
    title
        .trim()
        .trim_end_matches(" Announcement")
        .trim_end_matches(" announcement")
        .to_string()
}

fn first_paragraph(body: &str) -> String {
    let mut out = String::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if out.chars().count() + line.chars().count() > 400 {
            break;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(line);
    }
    out
}

fn provenance_append(source: &KnowledgeNote) -> String {
    format!(
        "\n## Sources\n\n- {}\n",
        format_wikilink(&source.title, &source.path)
    )
}

impl IngestionPlan {
    pub fn render(&self) -> String {
        let mut out = format!("SOURCE:\n{}\n", self.source_title);
        if self.duplicate {
            out.push_str("(duplicate source; not created again)\n");
        }
        if !self.candidates.is_empty() {
            out.push_str("\nCANDIDATES:\n");
            for candidate in &self.candidates {
                out.push_str(&format!(
                    "- {} [{}] id={}\n",
                    candidate.title,
                    candidate.kind.as_str(),
                    candidate.id
                ));
            }
        }
        if !self.create_notes.is_empty() {
            out.push_str("\nCREATE:\n");
            for create in &self.create_notes {
                out.push_str(&format!("- {}\n", create.title));
            }
        }
        if !self.update_notes.is_empty() {
            out.push_str("\nUPDATE:\n");
            for update in &self.update_notes {
                out.push_str(&format!("- {}\n", update.title));
            }
        }
        if !self.links.is_empty() {
            out.push_str("\nLINK:\n");
            for link in &self.links {
                out.push_str(&format!(
                    "- {} → {} ({})\n",
                    link.from,
                    link.to,
                    link.relation.as_str()
                ));
            }
        }
        if !self.conflicts.is_empty() {
            out.push_str("\nCONFLICTS:\n");
            for conflict in &self.conflicts {
                out.push_str(&format!("- {}: {}\n", conflict.title, conflict.reason));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexedVault;
    use std::fs;

    fn temp_paths() -> (std::path::PathBuf, std::path::PathBuf) {
        let id = uuid::Uuid::now_v7();
        (
            std::env::temp_dir().join(format!("crosspond-ingest-vault-{id}")),
            std::env::temp_dir().join(format!("crosspond-ingest-db-{id}.sqlite")),
        )
    }

    fn note(kind: NoteKind, title: &str, aliases: &[&str], body: &str) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind,
            title: title.into(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            tags: vec!["lab".into()],
            trust: TrustLevel::User,
            relations: Relations::default(),
            resource_kind: None,
            body: body.into(),
            relative_path: None,
            url: None,
            source_kind: None,
            source_status: None,
        }
    }

    fn lab_vault() -> (IndexedVault, std::path::PathBuf, std::path::PathBuf) {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        indexed
            .create_note(note(
                NoteKind::Knowledge,
                "Summer Assignment",
                &[],
                "# Summer Assignment\n\nCurrent laboratory homework.\n",
            ))
            .unwrap();
        indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab Wiki",
                &[],
                "# Lab Wiki\n\nInternal assignment pages.\n",
            ))
            .unwrap();
        indexed
            .create_note(note(
                NoteKind::Procedure,
                "Check Lab Assignment",
                &["研究室の課題確認"],
                "# Check Lab Assignment\n\nHow to retrieve assignments.\n",
            ))
            .unwrap();
        (indexed, vault, sqlite)
    }

    #[test]
    fn announcement_plans_updates_for_existing_lab_notes() {
        let (indexed, vault, sqlite) = lab_vault();
        let engine = IngestionEngine::new(&indexed);
        let plan = engine
            .ingest(SourceCapture {
                title: "New Laboratory Assignment".into(),
                body: "Please check the Summer Assignment on the Lab Wiki. Use Check Lab Assignment.\n".into(),
                url: None,
                source_kind: Some("announcement".into()),
            })
            .unwrap();
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.title == "Summer Assignment")
        );
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.title == "Lab Wiki")
        );
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.title == "Check Lab Assignment")
        );
        assert!(
            plan.update_notes
                .iter()
                .any(|update| update.title == "Summer Assignment")
        );
        assert!(
            plan.update_notes
                .iter()
                .any(|update| update.title == "Check Lab Assignment")
        );
        assert!(
            !plan
                .update_notes
                .iter()
                .any(|update| update.title == "Lab Wiki")
        );
        assert!(plan.links.iter().any(|link| {
            plan.candidates
                .iter()
                .any(|candidate| candidate.title == "Lab Wiki" && candidate.id == link.to)
        }));
        let rendered = plan.render();
        assert!(rendered.contains("Summer Assignment"));
        assert!(rendered.contains("Lab Wiki"));
        assert!(rendered.contains("Check Lab Assignment"));
        let outcome = engine.apply(&plan).unwrap();
        assert!(outcome.conflicts.is_empty());
        let summer = indexed.search("Summer Assignment", 4).unwrap();
        let note = indexed.read_indexed(&summer[0].id).unwrap();
        assert!(note.body.contains("Current laboratory homework"));
        assert!(note.body.contains("[[New Laboratory Assignment]]"));
        let log = fs::read_to_string(vault.join("Log.md")).unwrap();
        assert!(log.contains("[[New Laboratory Assignment]]"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn duplicate_fingerprint_does_not_create_a_second_source() {
        let (indexed, vault, sqlite) = lab_vault();
        let engine = IngestionEngine::new(&indexed);
        let capture = SourceCapture {
            title: "New Laboratory Assignment".into(),
            body: "Please check the Summer Assignment on the Lab Wiki.\n".into(),
            url: Some("https://example.invalid/lab/assignment".into()),
            source_kind: Some("url".into()),
        };
        let first = engine.ingest(capture.clone()).unwrap();
        let second = engine.ingest(capture).unwrap();
        assert!(second.duplicate);
        assert_eq!(first.source_id, second.source_id);
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn secrets_are_rejected_and_hash_conflicts_are_reported() {
        let (indexed, vault, sqlite) = lab_vault();
        let engine = IngestionEngine::new(&indexed);
        let err = engine
            .ingest(SourceCapture {
                title: "Keys".into(),
                body: "api_key=sk-test-should-not-be-stored\n".into(),
                url: None,
                source_kind: None,
            })
            .unwrap_err();
        assert!(err.to_string().contains("secret"));
        let plan = engine
            .ingest(SourceCapture {
                title: "New Laboratory Assignment".into(),
                body: "Update the Summer Assignment with a later due date.\n".into(),
                url: None,
                source_kind: None,
            })
            .unwrap();
        let summer = indexed.search("Summer Assignment", 4).unwrap()[0].clone();
        let path = vault.join(indexed.read_indexed(&summer.id).unwrap().path);
        fs::write(&path, "# Summer Assignment\n\nEdited in Obsidian.\n").unwrap();
        let outcome = engine.apply(&plan).unwrap();
        assert!(
            outcome
                .conflicts
                .iter()
                .any(|conflict| conflict.title == "Summer Assignment")
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("Edited in Obsidian"));
        assert!(!text.contains("[[New Laboratory Assignment]]"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn unread_source_is_processed_into_existing_knowledge() {
        let (indexed, vault, sqlite) = lab_vault();
        let engine = IngestionEngine::new(&indexed);
        let saved = engine
            .save_unread(SourceCapture {
                title: "New Laboratory Assignment".into(),
                body: "Please check the Summer Assignment on the Lab Wiki.\n".into(),
                url: Some("https://example.invalid/lab/later".into()),
                source_kind: Some("url".into()),
            })
            .unwrap();
        assert_eq!(saved.source_status, Some(SourceStatus::Unread));
        let id = saved.id.clone().unwrap().to_string();
        let plan = engine.propose(&id).unwrap();
        assert!(
            plan.update_notes
                .iter()
                .any(|update| update.title == "Summer Assignment")
        );
        engine.apply(&plan).unwrap();
        let processed = engine.set_status(&id, SourceStatus::Processed).unwrap();
        assert_eq!(processed.source_status, Some(SourceStatus::Processed));
        let archived = engine.set_status(&id, SourceStatus::Archived).unwrap();
        assert_eq!(archived.source_status, Some(SourceStatus::Archived));
        let summer = indexed.search("Summer Assignment", 4).unwrap();
        let note = indexed.read_indexed(&summer[0].id).unwrap();
        assert!(note.body.contains("[[New Laboratory Assignment]]"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn ingest_writes_obsidian_safe_wikilinks_for_illegal_titles() {
        let (indexed, vault, sqlite) = lab_vault();
        let engine = IngestionEngine::new(&indexed);
        let plan = engine
            .ingest(SourceCapture {
                title: "cordiverse/paper: A Programming Paradigm".into(),
                body: "Please check the Summer Assignment.\n".into(),
                url: None,
                source_kind: Some("url".into()),
            })
            .unwrap();
        let source = indexed.read_indexed(&plan.source_id.to_string()).unwrap();
        assert_eq!(
            source.path,
            std::path::PathBuf::from("sources/cordiverse-paper- A Programming Paradigm.md")
        );
        let expected = format_wikilink(&source.title, &source.path);
        assert!(
            plan.create_notes
                .iter()
                .any(|create| create.body.contains(&expected))
                || plan
                    .update_notes
                    .iter()
                    .any(|update| update.append.contains(&expected))
        );
        assert!(
            !plan.create_notes.iter().any(|create| create
                .body
                .contains("[[cordiverse/paper: A Programming Paradigm]]"))
                && !plan.update_notes.iter().any(|update| update
                    .append
                    .contains("[[cordiverse/paper: A Programming Paradigm]]"))
        );
        engine.apply(&plan).unwrap();
        let index = fs::read_to_string(vault.join("Index.md")).unwrap();
        assert!(index.contains(&expected));
        assert!(!index.contains("[[cordiverse/paper: A Programming Paradigm]]"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }
}
