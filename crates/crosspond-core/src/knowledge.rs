use std::collections::HashSet;
use std::sync::Arc;

use crosspond_knowledge::{IndexedVault, index_note_id, search_queries};
use crosspond_tools::{KnowledgeBackend, KnowledgeEdge, KnowledgeHit, KnowledgeRecord};

pub(crate) struct VaultKnowledge(pub Arc<IndexedVault>);

impl KnowledgeBackend for VaultKnowledge {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String> {
        collect_hits(&self.0, query, limit, |vault, q, n| vault.search(q, n))
    }

    fn read(&self, id: &str) -> Result<KnowledgeRecord, String> {
        let note = self.0.read_indexed(id).map_err(|err| err.to_string())?;
        Ok(KnowledgeRecord {
            id: index_note_id(&note),
            title: note.title,
            kind: note.kind.as_str().into(),
            aliases: note.aliases,
            tags: note.tags,
            body: note.body,
            path: note.path.display().to_string(),
            credential_ref: note.credential_ref,
        })
    }

    fn neighbors(&self, id: &str) -> Result<Vec<KnowledgeEdge>, String> {
        map_edges(
            &self.0,
            self.0
                .index()
                .neighbors(id)
                .map_err(|err| err.to_string())?,
            false,
        )
    }

    fn backlinks(&self, id: &str) -> Result<Vec<KnowledgeEdge>, String> {
        map_edges(
            &self.0,
            self.0
                .index()
                .backlinks(id)
                .map_err(|err| err.to_string())?,
            true,
        )
    }

    fn find_procedure(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String> {
        collect_hits(&self.0, query, limit, |vault, q, n| {
            vault.find_procedure(q, n)
        })
    }

    fn ingest(
        &self,
        title: &str,
        body: &str,
        url: Option<&str>,
        source_kind: Option<&str>,
    ) -> Result<String, String> {
        let engine = crosspond_knowledge::IngestionEngine::new(&self.0);
        let plan = engine
            .ingest(crosspond_knowledge::SourceCapture {
                title: title.into(),
                body: body.into(),
                url: url.map(str::to_string),
                source_kind: source_kind.map(str::to_string),
            })
            .map_err(|err| err.to_string())?;
        let outcome = engine.apply(&plan).map_err(|err| err.to_string())?;
        Ok(render_ingestion(&plan, &outcome))
    }

    fn propose_update(&self, id: &str) -> Result<String, String> {
        let engine = crosspond_knowledge::IngestionEngine::new(&self.0);
        let plan = engine.propose(id).map_err(|err| err.to_string())?;
        let outcome = engine.apply(&plan).map_err(|err| err.to_string())?;
        let _ = engine.set_status(id, crosspond_knowledge::SourceStatus::Processed);
        Ok(render_ingestion(&plan, &outcome))
    }

    fn save_unread(
        &self,
        title: &str,
        body: &str,
        url: Option<&str>,
        source_kind: Option<&str>,
    ) -> Result<String, String> {
        let engine = crosspond_knowledge::IngestionEngine::new(&self.0);
        let note = engine
            .save_unread(crosspond_knowledge::SourceCapture {
                title: title.into(),
                body: body.into(),
                url: url.map(str::to_string),
                source_kind: source_kind.map(str::to_string),
            })
            .map_err(|err| err.to_string())?;
        let id = note
            .id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        let status = note
            .source_status
            .unwrap_or(crosspond_knowledge::SourceStatus::Unread)
            .as_str();
        Ok(format!(
            "Saved unread source: {}\nid={id}\nstatus={status}\n",
            note.title
        ))
    }

    fn archive_source(&self, id: &str) -> Result<String, String> {
        let engine = crosspond_knowledge::IngestionEngine::new(&self.0);
        let note = engine
            .set_status(id, crosspond_knowledge::SourceStatus::Archived)
            .map_err(|err| err.to_string())?;
        Ok(format!("Archived source {}\n", note.title))
    }
}

fn render_ingestion(
    plan: &crosspond_knowledge::IngestionPlan,
    outcome: &crosspond_knowledge::IngestionOutcome,
) -> String {
    let mut text = plan.render();
    if !outcome.created.is_empty() {
        text.push_str("\nAPPLIED CREATE:\n");
        for id in &outcome.created {
            text.push_str(&format!("- {id}\n"));
        }
    }
    if !outcome.updated.is_empty() {
        text.push_str("\nAPPLIED UPDATE:\n");
        for id in &outcome.updated {
            text.push_str(&format!("- {id}\n"));
        }
    }
    if !outcome.conflicts.is_empty() {
        text.push_str("\nAPPLY CONFLICTS:\n");
        for conflict in &outcome.conflicts {
            text.push_str(&format!("- {}: {}\n", conflict.title, conflict.reason));
        }
    }
    text
}

fn collect_hits(
    vault: &IndexedVault,
    query: &str,
    limit: usize,
    search: impl Fn(
        &IndexedVault,
        &str,
        usize,
    ) -> Result<Vec<crosspond_knowledge::SearchHit>, crosspond_knowledge::VaultError>,
) -> Result<Vec<KnowledgeHit>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    // Prefer stripped forms ("研究室の課題確認して" → "研究室の課題確認") so alias hits win.
    for q in search_queries(query).into_iter().rev() {
        for hit in search(vault, &q, limit).map_err(|err| err.to_string())? {
            if seen.insert(hit.id.clone()) {
                out.push(to_hit(vault, hit));
            }
        }
    }
    out.truncate(limit);
    Ok(out)
}

fn to_hit(vault: &IndexedVault, hit: crosspond_knowledge::SearchHit) -> KnowledgeHit {
    let snippet = vault
        .read_indexed(&hit.id)
        .ok()
        .map(|note| first_line(&note.body))
        .unwrap_or_default();
    KnowledgeHit {
        id: hit.id,
        title: hit.title,
        kind: hit.kind.as_str().into(),
        snippet,
    }
}

fn map_edges(
    vault: &IndexedVault,
    links: Vec<crosspond_knowledge::IndexedLink>,
    backlinks: bool,
) -> Result<Vec<KnowledgeEdge>, String> {
    let mut edges = Vec::new();
    for link in links {
        let title_id = if backlinks {
            &link.source_id
        } else {
            &link.target_id
        };
        let title = vault
            .index()
            .lookup(title_id)
            .ok()
            .flatten()
            .map(|hit| hit.title);
        edges.push(KnowledgeEdge {
            source_id: link.source_id,
            target_id: link.target_id,
            relation: link.relation_type,
            title,
        });
    }
    Ok(edges)
}

fn first_line(body: &str) -> String {
    body.lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SavedUnread {
    SelectedText,
    Page,
    File { name: String },
}

pub(crate) fn save_ambient_read_later(
    vault: &IndexedVault,
    context: &crate::context::ContextCapsule,
    staged: &[crate::context::StagedInput],
) -> Vec<SavedUnread> {
    let engine = crosspond_knowledge::IngestionEngine::new(vault);
    let mut saved = Vec::new();
    if let Some(url) = context
        .page_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        let title = context
            .focused_window
            .as_ref()
            .and_then(|window| window.title.as_deref())
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| page_title_from_url(url));
        if save_capture(
            &engine,
            &title,
            "Unread page captured from the frontmost browser.\n",
            Some(url),
            "url",
        ) {
            saved.push(SavedUnread::Page);
        }
    }
    if let Some(text) = context
        .selected_text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        let title = selection_title(text);
        if save_capture(&engine, &title, text, None, "text") {
            saved.push(SavedUnread::SelectedText);
        }
    }
    let files: Vec<(String, std::path::PathBuf)> = if !staged.is_empty() {
        staged
            .iter()
            .map(|file| {
                let name = std::path::Path::new(&file.relative)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("file")
                    .to_string();
                (name, file.original.clone())
            })
            .collect()
    } else {
        context
            .selected_files
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| (name.to_string(), path.clone()))
            })
            .collect()
    };
    for (name, path) in files {
        let (kind, body) = file_source_body(&name, &path);
        if save_capture(&engine, &name, &body, None, kind) {
            saved.push(SavedUnread::File { name });
        }
    }
    saved
}

pub(crate) fn render_read_later_summary(saved: &[SavedUnread]) -> String {
    let mut lines = Vec::new();
    for item in saved {
        match item {
            SavedUnread::SelectedText => {
                lines.push("Saved selected text as an unread Source.".into());
            }
            SavedUnread::Page => {
                lines.push("Saved the current page as an unread Source.".into());
            }
            SavedUnread::File { name } => {
                lines.push(format!("Saved {name} as an unread Source."));
            }
        }
    }
    if lines.is_empty() {
        "Nothing to save for later.".into()
    } else {
        lines.join("\n")
    }
}

fn save_capture(
    engine: &crosspond_knowledge::IngestionEngine<'_>,
    title: &str,
    body: &str,
    url: Option<&str>,
    source_kind: &str,
) -> bool {
    let mut title = title.trim().to_string();
    if title.is_empty() {
        title = "Unread source".into();
    }
    for attempt in 0..3 {
        let candidate = if attempt == 0 {
            title.clone()
        } else {
            format!("{title} ({attempt})")
        };
        match engine.save_unread(crosspond_knowledge::SourceCapture {
            title: candidate,
            body: body.into(),
            url: url.map(str::to_string),
            source_kind: Some(source_kind.into()),
        }) {
            Ok(_) => return true,
            Err(crosspond_knowledge::VaultError::DuplicatePath(_)) => continue,
            Err(_) => return false,
        }
    }
    false
}

fn selection_title(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Selected text");
    let mut title: String = line.chars().take(48).collect();
    if line.chars().count() > 48 {
        title.push('…');
    }
    title
}

fn page_title_from_url(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("Current page")
        .split('?')
        .next()
        .unwrap_or("Current page")
        .to_string()
}

fn file_source_body(name: &str, path: &std::path::Path) -> (&'static str, String) {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "pdf" {
        return ("pdf", format!("Dropped PDF: {name}\n"));
    }
    if matches!(
        ext.as_str(),
        "md" | "txt" | "csv" | "json" | "html" | "htm" | "markdown"
    ) && let Ok(bytes) = std::fs::read(path)
    {
        let text = String::from_utf8_lossy(&bytes);
        let mut body: String = text
            .chars()
            .take(crate::context::MAX_AMBIENT_TEXT_CHARS)
            .collect();
        if text.chars().count() > crate::context::MAX_AMBIENT_TEXT_CHARS {
            body.push_str("\n… truncated");
        }
        return ("file", body);
    }
    ("file", format!("Local document: {name}\n"))
}
