use crate::index::{IndexedVault, SearchHit};
use crate::model::NoteKind;
use crate::vault::VaultError;

use super::query::search_queries;
use super::ranking::merge_hits;

const MAX_PROCEDURES: usize = 3;
const MAX_RESOURCES: usize = 6;
const MAX_KNOWLEDGE: usize = 4;
const MAX_ACTIVITY: usize = 3;
const SNIPPET_CHARS: usize = 160;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeContextRequest {
    pub prompt: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeSummary {
    pub id: String,
    pub title: String,
    pub kind: NoteKind,
    pub snippet: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivitySummary {
    pub id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KnowledgeBrief {
    pub procedures: Vec<KnowledgeSummary>,
    pub resources: Vec<KnowledgeSummary>,
    pub knowledge: Vec<KnowledgeSummary>,
    pub recent_activity: Vec<ActivitySummary>,
}

impl KnowledgeBrief {
    pub fn is_empty(&self) -> bool {
        self.procedures.is_empty()
            && self.resources.is_empty()
            && self.knowledge.is_empty()
            && self.recent_activity.is_empty()
    }

    pub fn render(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::from("Relevant Knowledge\n");
        push_section(&mut out, "Procedure", &self.procedures);
        push_section(&mut out, "Resources", &self.resources);
        push_section(&mut out, "Knowledge", &self.knowledge);
        if !self.recent_activity.is_empty() {
            out.push_str("\nRecent Activity:\n");
            for item in &self.recent_activity {
                out.push_str(&format!("- {}\n", item.title));
                if !item.snippet.is_empty() {
                    out.push_str(&format!("  {}\n", item.snippet));
                }
            }
        }
        out
    }
}

pub struct KnowledgeRouter<'a> {
    vault: &'a IndexedVault,
}

impl<'a> KnowledgeRouter<'a> {
    pub fn new(vault: &'a IndexedVault) -> Self {
        Self { vault }
    }

    pub fn route(&self, request: &KnowledgeContextRequest) -> Result<KnowledgeBrief, VaultError> {
        let mut batches = Vec::new();
        for query in search_queries(&request.prompt) {
            batches.push(self.vault.search(&query, 16)?);
        }
        let hits = merge_hits(&request.prompt, batches);
        let mut brief = KnowledgeBrief::default();
        for hit in &hits {
            match hit.kind {
                NoteKind::Procedure if brief.procedures.len() < MAX_PROCEDURES => {
                    brief.procedures.push(self.summarize(hit)?);
                }
                NoteKind::Resource if brief.resources.len() < MAX_RESOURCES => {
                    brief.resources.push(self.summarize(hit)?);
                }
                NoteKind::Knowledge | NoteKind::Synthesis
                    if brief.knowledge.len() < MAX_KNOWLEDGE =>
                {
                    brief.knowledge.push(self.summarize(hit)?);
                }
                _ => {}
            }
        }
        self.expand_procedure_resources(&mut brief)?;
        for hit in self
            .vault
            .index()
            .recent_kind(NoteKind::Activity, MAX_ACTIVITY)?
        {
            brief.recent_activity.push(ActivitySummary {
                snippet: self.snippet_for(&hit)?,
                id: hit.id,
                title: hit.title,
            });
        }
        Ok(brief)
    }

    fn expand_procedure_resources(&self, brief: &mut KnowledgeBrief) -> Result<(), VaultError> {
        let procedure_ids: Vec<String> = brief
            .procedures
            .iter()
            .map(|item| item.id.clone())
            .collect();
        for id in procedure_ids {
            for link in self.vault.index().neighbors(&id)? {
                if !matches!(
                    link.relation_type.as_str(),
                    "uses" | "requires" | "related" | "wikilink"
                ) {
                    continue;
                }
                if brief.resources.iter().any(|item| item.id == link.target_id)
                    || brief.resources.len() >= MAX_RESOURCES
                {
                    continue;
                }
                let Some(hit) = self.vault.index().lookup(&link.target_id)? else {
                    continue;
                };
                if hit.kind == NoteKind::Resource {
                    brief.resources.push(self.summarize(&hit)?);
                }
            }
        }
        Ok(())
    }

    fn summarize(&self, hit: &SearchHit) -> Result<KnowledgeSummary, VaultError> {
        Ok(KnowledgeSummary {
            id: hit.id.clone(),
            title: hit.title.clone(),
            kind: hit.kind,
            snippet: self.snippet_for(hit)?,
        })
    }

    fn snippet_for(&self, hit: &SearchHit) -> Result<String, VaultError> {
        let note = self.vault.read_indexed(&hit.id)?;
        Ok(snippet(&note.body))
    }
}

fn snippet(body: &str) -> String {
    let line = body
        .lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let mut out = String::new();
    for ch in line.chars() {
        if out.chars().count() >= SNIPPET_CHARS {
            break;
        }
        out.push(ch);
    }
    out
}

fn push_section(out: &mut String, title: &str, items: &[KnowledgeSummary]) {
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("\n{title}:\n"));
    for item in items {
        out.push_str(&format!("- {}  id={}\n", item.title, item.id));
        if !item.snippet.is_empty() {
            out.push_str(&format!("  {}\n", item.snippet));
        }
    }
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
            std::env::temp_dir().join(format!("crosspond-brief-vault-{id}")),
            std::env::temp_dir().join(format!("crosspond-brief-db-{id}.sqlite")),
        )
    }

    fn note(
        kind: NoteKind,
        title: &str,
        aliases: &[&str],
        body: &str,
        relations: Relations,
    ) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind,
            title: title.into(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            tags: vec!["lab".into()],
            trust: TrustLevel::User,
            relations,
            resource_kind: None,
            body: body.into(),
            relative_path: None,
        }
    }

    fn lab_vault() -> (IndexedVault, std::path::PathBuf, std::path::PathBuf) {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let vpn = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab VPN",
                &["研究室VPN"],
                "# Lab VPN\n\nWireGuard profile for the laboratory network.\n",
                Relations::default(),
            ))
            .unwrap();
        let wiki = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab Wiki",
                &[],
                "# Lab Wiki\n\nInternal assignment pages.\n",
                Relations::default(),
            ))
            .unwrap();
        let files = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab File Server",
                &[],
                "# Lab File Server\n\nsmb://lab-files\n",
                Relations::default(),
            ))
            .unwrap();
        let mut relations = Relations::default();
        relations.requires.push(vpn.id.clone().unwrap());
        relations.uses.push(wiki.id.clone().unwrap());
        relations.uses.push(files.id.clone().unwrap());
        indexed
            .create_note(note(
                NoteKind::Procedure,
                "Check Lab Assignment",
                &["研究室の課題確認"],
                "# Check Lab Assignment\n\nHow to retrieve current laboratory assignments.\n\nNeeds [[Lab VPN]], [[Lab Wiki]], and [[Lab File Server]].\n",
                relations,
            ))
            .unwrap();
        (indexed, vault, sqlite)
    }

    #[test]
    fn command_prompt_finds_lab_procedure_before_acting() {
        let (indexed, vault, sqlite) = lab_vault();
        let brief = KnowledgeRouter::new(&indexed)
            .route(&KnowledgeContextRequest {
                prompt: "研究室の課題確認して".into(),
            })
            .unwrap();
        assert_eq!(brief.procedures[0].title, "Check Lab Assignment");
        let rendered = brief.render();
        assert!(rendered.contains("Check Lab Assignment"));
        assert!(rendered.contains(&brief.procedures[0].id));
        assert!(rendered.contains("Lab VPN"));
        assert!(rendered.contains("Lab Wiki"));
        assert!(rendered.contains("Lab File Server"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn question_prefers_resource_over_procedure() {
        let (indexed, vault, sqlite) = lab_vault();
        let brief = KnowledgeRouter::new(&indexed)
            .route(&KnowledgeContextRequest {
                prompt: "研究室のVPNって何?".into(),
            })
            .unwrap();
        assert!(brief.resources.iter().any(|item| item.title == "Lab VPN"));
        assert!(
            brief.procedures.is_empty()
                || brief.resources[0].title == "Lab VPN"
                || brief
                    .knowledge
                    .iter()
                    .any(|item| item.title.contains("VPN"))
        );
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }
}
