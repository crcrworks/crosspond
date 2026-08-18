use std::fs;
use std::path::Path;

use super::VaultError;
use super::paths::format_wikilink;
use crate::model::{KnowledgeNote, NoteKind};

pub fn rebuild_index(root: &Path, notes: &[KnowledgeNote]) -> Result<(), VaultError> {
    let mut procedures = Vec::new();
    let mut resources = Vec::new();
    let mut knowledge = Vec::new();
    let mut sources = Vec::new();
    let mut activity = Vec::new();
    let mut syntheses = Vec::new();
    for note in notes {
        if note.id.is_none() {
            continue;
        }
        let link = format!("- {}", format_wikilink(&note.title, &note.path));
        match note.kind {
            NoteKind::Procedure => procedures.push(link),
            NoteKind::Resource => resources.push(link),
            NoteKind::Knowledge => knowledge.push(link),
            NoteKind::Source => sources.push(link),
            NoteKind::Activity => activity.push(link),
            NoteKind::Synthesis => syntheses.push(link),
        }
    }
    procedures.sort();
    resources.sort();
    knowledge.sort();
    sources.sort();
    activity.sort();
    syntheses.sort();
    let mut sections = vec!["# Knowledge Index".to_string(), String::new()];
    push_section(&mut sections, "Procedures", &procedures);
    push_section(&mut sections, "Resources", &resources);
    push_section(&mut sections, "Knowledge", &knowledge);
    push_section(&mut sections, "Syntheses", &syntheses);
    push_section(&mut sections, "Sources", &sources);
    push_section(&mut sections, "Activity", &activity);
    let rendered = sections.join("\n");
    let path = root.join("Index.md");
    if fs::read_to_string(&path).ok().as_deref() == Some(rendered.as_str()) {
        return Ok(());
    }
    fs::write(path, rendered).map_err(|err| VaultError::Io(err.to_string()))
}

pub fn append_log(root: &Path, heading: &str, lines: &[String]) -> Result<(), VaultError> {
    let path = root.join("Log.md");
    let mut existing = if path.exists() {
        fs::read_to_string(&path).map_err(|err| VaultError::Io(err.to_string()))?
    } else {
        "# Knowledge Log\n".into()
    };
    if !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push('\n');
    existing.push_str(&format!("## {heading}\n\n"));
    for line in lines {
        existing.push_str(line);
        existing.push('\n');
    }
    fs::write(path, existing).map_err(|err| VaultError::Io(err.to_string()))
}

fn push_section(out: &mut Vec<String>, title: &str, items: &[String]) {
    out.push(format!("## {title}"));
    out.push(String::new());
    if items.is_empty() {
        out.push("_None yet._".into());
    } else {
        out.extend(items.iter().cloned());
    }
    out.push(String::new());
}
