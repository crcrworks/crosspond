use std::str::FromStr;

use time::OffsetDateTime;

use crate::index::IndexedVault;
use crate::model::{
    KnowledgeId, KnowledgeNote, KnowledgePatch, NewKnowledgeNote, NoteKind, Relations, TrustLevel,
};
use crate::vault::{VaultError, VaultRepository};

const RESULT_CHARS: usize = 1500;
const TITLE_CHARS: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityStatus {
    Completed,
    Failed,
    Cancelled,
}

impl ActivityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityRecord {
    pub title: String,
    pub result: String,
    pub status: ActivityStatus,
    pub procedure: Option<KnowledgeId>,
    pub resources: Vec<KnowledgeId>,
    pub knowledge: Vec<KnowledgeId>,
    pub sources: Vec<KnowledgeId>,
    pub actions: Vec<String>,
    pub artifacts: Vec<String>,
}

pub struct ActivityRecorder<'a> {
    vault: &'a IndexedVault,
}

impl<'a> ActivityRecorder<'a> {
    pub fn new(vault: &'a IndexedVault) -> Self {
        Self { vault }
    }

    pub fn record(&self, record: ActivityRecord) -> Result<KnowledgeNote, VaultError> {
        let now = OffsetDateTime::now_utc();
        let title = unique_title(self.vault, &record.title, &now);
        let body = render_body(self.vault, &record);
        let mut relations = Relations::default();
        if let Some(procedure) = &record.procedure {
            relations.produced_by.push(procedure.clone());
        }
        relations.uses = record.resources.clone();
        relations.mentions = record.knowledge.clone();
        let written = self.vault.create_note(NewKnowledgeNote {
            kind: NoteKind::Activity,
            title,
            aliases: Vec::new(),
            tags: Vec::new(),
            trust: TrustLevel::Derived,
            relations,
            resource_kind: None,
            body,
            relative_path: None,
            url: None,
            source_kind: None,
            source_status: None,
        })?;
        if record.status == ActivityStatus::Completed
            && let Some(procedure) = &record.procedure
        {
            let _ = self.touch_last_verified(procedure, &now);
        }
        Ok(written)
    }

    fn touch_last_verified(
        &self,
        id: &KnowledgeId,
        now: &OffsetDateTime,
    ) -> Result<(), VaultError> {
        let note = self.vault.read_indexed(id.as_str())?;
        let day = format_day(now);
        self.vault.apply_patch(KnowledgePatch {
            id: id.clone(),
            expected_hash: note.content_hash,
            title: None,
            aliases: None,
            tags: None,
            trust: None,
            relations: None,
            last_verified: Some(day),
            source_status: None,
            body: None,
        })?;
        Ok(())
    }
}

fn unique_title(vault: &IndexedVault, base: &str, now: &OffsetDateTime) -> String {
    let base = truncate(base, TITLE_CHARS);
    let day = format_day(now);
    let primary = format!("{base} — {day}");
    if note_path_free(vault, &primary, now) {
        return primary;
    }
    format!("{base} — {day} {:02}.{:02}", now.hour(), now.minute())
}

fn note_path_free(vault: &IndexedVault, title: &str, now: &OffsetDateTime) -> bool {
    match crate::vault::default_relative_path(NoteKind::Activity, title, now) {
        Ok(relative) => !vault.repository().root().join(relative).exists(),
        Err(_) => true,
    }
}

fn render_body(vault: &IndexedVault, record: &ActivityRecord) -> String {
    let mut body = format!("# {}\n\n## Result\n\n", record.title);
    let result = sanitize_result(&record.result);
    if result.is_empty() {
        body.push_str("(no summary)\n");
    } else {
        body.push_str(&result);
        body.push('\n');
    }
    if let Some(procedure) = &record.procedure {
        body.push_str("\n## Procedure\n\n");
        body.push_str(&format!("- {}\n", wiki_item(vault, procedure)));
    }
    if !record.resources.is_empty() {
        body.push_str("\n## Resources\n\n");
        for id in &record.resources {
            body.push_str(&format!("- {}\n", wiki_item(vault, id)));
        }
    }
    if !record.knowledge.is_empty() {
        body.push_str("\n## Knowledge\n\n");
        for id in &record.knowledge {
            body.push_str(&format!("- {}\n", wiki_item(vault, id)));
        }
    }
    if !record.sources.is_empty() {
        body.push_str("\n## Sources\n\n");
        for id in &record.sources {
            body.push_str(&format!("- {}\n", wiki_item(vault, id)));
        }
    }
    if !record.actions.is_empty() {
        body.push_str("\n## Actions\n\n");
        for action in &record.actions {
            body.push_str(&format!("- {}\n", action.trim()));
        }
    }
    if !record.artifacts.is_empty() {
        body.push_str("\n## Artifacts\n\n");
        for artifact in &record.artifacts {
            body.push_str(&format!("- `{artifact}`\n"));
        }
    }
    body.push_str(&format!("\nStatus: {}\n", record.status.as_str()));
    body
}

fn wiki_item(vault: &IndexedVault, id: &KnowledgeId) -> String {
    match vault.index().lookup(id.as_str()) {
        Ok(Some(hit)) => format!("[[{}]]", hit.title),
        _ => format!("`{}`", id.as_str()),
    }
}

fn sanitize_result(summary: &str) -> String {
    let mut out = String::new();
    for line in summary.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{')
            || trimmed.contains("\"arguments\"")
            || trimmed.contains("chain-of-thought")
        {
            continue;
        }
        if out.chars().count() + trimmed.chars().count() > RESULT_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(trimmed);
    }
    out
}

fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    let mut out: String = trimmed.chars().take(max).collect();
    if trimmed.chars().count() > max {
        out.push('…');
    }
    if out.is_empty() {
        "Untitled".into()
    } else {
        out
    }
}

fn format_day(now: &OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    )
}

pub fn parse_note_id(value: &str) -> Option<KnowledgeId> {
    KnowledgeId::from_str(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexedVault;
    use crate::model::{NewKnowledgeNote, NoteKind, Relations, TrustLevel};
    use std::fs;

    fn temp_paths() -> (std::path::PathBuf, std::path::PathBuf) {
        let id = uuid::Uuid::now_v7();
        (
            std::env::temp_dir().join(format!("crosspond-activity-vault-{id}")),
            std::env::temp_dir().join(format!("crosspond-activity-db-{id}.sqlite")),
        )
    }

    fn note(kind: NoteKind, title: &str, body: &str) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind,
            title: title.into(),
            aliases: Vec::new(),
            tags: Vec::new(),
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

    #[test]
    fn records_readable_history_linked_to_procedure() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let procedure = indexed
            .create_note(note(
                NoteKind::Procedure,
                "Check Lab Assignment",
                "# Check Lab Assignment\n\nEnable the VPN, then open the wiki.\n",
            ))
            .unwrap();
        let vpn = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab VPN",
                "# Lab VPN\n\nWireGuard profile.\n",
            ))
            .unwrap();
        let written = ActivityRecorder::new(&indexed)
            .record(ActivityRecord {
                title: "Check Lab Assignment".into(),
                result: "Found one new assignment.\n{\"arguments\":\"secret\"}\n".into(),
                status: ActivityStatus::Completed,
                procedure: procedure.id.clone(),
                resources: vec![vpn.id.clone().unwrap()],
                knowledge: Vec::new(),
                sources: Vec::new(),
                actions: vec!["Connected to Lab VPN".into()],
                artifacts: vec!["assignment.pdf".into()],
            })
            .unwrap();
        assert_eq!(written.kind, NoteKind::Activity);
        let path = written.path.to_string_lossy();
        assert!(path.starts_with("history/"));
        assert!(path.ends_with(".md"));
        let text = fs::read_to_string(vault.join(&written.path)).unwrap();
        assert!(text.contains("## Result"));
        assert!(text.contains("Found one new assignment"));
        assert!(!text.contains("arguments"));
        assert!(text.contains("[[Check Lab Assignment]]"));
        assert!(text.contains("[[Lab VPN]]"));
        assert!(text.contains("`assignment.pdf`"));
        assert!(text.contains("Status: completed"));
        assert_eq!(
            written.relations.produced_by[0],
            procedure.id.clone().unwrap()
        );
        let refreshed = indexed
            .read_indexed(procedure.id.as_ref().unwrap().as_str())
            .unwrap();
        assert!(refreshed.last_verified.is_some());
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }
}
