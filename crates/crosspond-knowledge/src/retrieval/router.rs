use crate::index::{IndexedVault, SearchHit};
use crate::model::{KnowledgeNote, NoteKind};
use crate::vault::VaultError;

use super::query::{looks_like_command, search_queries};
use super::ranking::merge_hits;

const MAX_PROCEDURES: usize = 3;
const MAX_RESOURCES: usize = 6;
const MAX_KNOWLEDGE: usize = 4;
const MAX_ACTIVITY: usize = 3;
const SNIPPET_CHARS: usize = 160;
const STALE_VERIFIED_DAYS: i64 = 30;

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
    pub last_verified: Option<String>,
    pub resource_kind: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivitySummary {
    pub id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcedureFollow {
    pub procedure: KnowledgeSummary,
    pub requires: Vec<KnowledgeSummary>,
    pub uses: Vec<KnowledgeSummary>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KnowledgeBrief {
    pub procedures: Vec<KnowledgeSummary>,
    pub resources: Vec<KnowledgeSummary>,
    pub knowledge: Vec<KnowledgeSummary>,
    pub recent_activity: Vec<ActivitySummary>,
    pub follow: Option<ProcedureFollow>,
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
        if let Some(follow) = &self.follow {
            push_follow(&mut out, follow);
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
        if looks_like_command(&request.prompt)
            && let Some(procedure) = brief.procedures.first().cloned()
        {
            let follow = self.build_follow(&procedure)?;
            brief.resources = ordered_resources(&follow, brief.resources.drain(..).collect());
            brief.follow = Some(follow);
        }
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
            let mut links = self.vault.index().neighbors(&id)?;
            links.sort_by_key(|link| relation_priority(&link.relation_type));
            for link in links {
                if relation_priority(&link.relation_type) > 3 {
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

    fn build_follow(&self, procedure: &KnowledgeSummary) -> Result<ProcedureFollow, VaultError> {
        let mut follow = ProcedureFollow {
            procedure: procedure.clone(),
            requires: Vec::new(),
            uses: Vec::new(),
        };
        let mut seen = std::collections::HashSet::from([procedure.id.clone()]);
        let mut links = self.vault.index().neighbors(&procedure.id)?;
        links.sort_by_key(|link| relation_priority(&link.relation_type));
        for link in links {
            let requires = link.relation_type == "requires";
            let uses = matches!(link.relation_type.as_str(), "uses" | "related" | "wikilink");
            if !requires && !uses {
                continue;
            }
            if !seen.insert(link.target_id.clone()) {
                continue;
            }
            if follow.requires.len() + follow.uses.len() >= MAX_RESOURCES {
                break;
            }
            let Some(hit) = self.vault.index().lookup(&link.target_id)? else {
                continue;
            };
            if hit.kind != NoteKind::Resource {
                continue;
            }
            let summary = self.summarize(&hit)?;
            if requires {
                follow.requires.push(summary);
            } else {
                follow.uses.push(summary);
            }
        }
        Ok(follow)
    }

    fn summarize(&self, hit: &SearchHit) -> Result<KnowledgeSummary, VaultError> {
        let note = self.vault.read_indexed(&hit.id)?;
        Ok(summary_from_note(&hit.id, &note))
    }

    fn snippet_for(&self, hit: &SearchHit) -> Result<String, VaultError> {
        let note = self.vault.read_indexed(&hit.id)?;
        Ok(snippet(&note.body))
    }
}

fn summary_from_note(id: &str, note: &KnowledgeNote) -> KnowledgeSummary {
    KnowledgeSummary {
        id: id.into(),
        title: note.title.clone(),
        kind: note.kind,
        snippet: snippet(&note.body),
        last_verified: note.last_verified.clone(),
        resource_kind: note.resource_kind.clone(),
    }
}

fn ordered_resources(
    follow: &ProcedureFollow,
    extras: Vec<KnowledgeSummary>,
) -> Vec<KnowledgeSummary> {
    let mut ordered = follow.requires.clone();
    ordered.extend(follow.uses.iter().cloned());
    for extra in extras {
        if !ordered.iter().any(|item| item.id == extra.id) {
            ordered.push(extra);
        }
    }
    ordered.truncate(MAX_RESOURCES);
    ordered
}

fn relation_priority(relation: &str) -> u8 {
    match relation {
        "requires" => 0,
        "uses" => 1,
        "related" => 2,
        "wikilink" => 3,
        _ => 4,
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
        out.push_str(&format!("- {}\n", summary_line(item)));
        if !item.snippet.is_empty() {
            out.push_str(&format!("  {}\n", item.snippet));
        }
    }
}

fn push_follow(out: &mut String, follow: &ProcedureFollow) {
    out.push_str("\nHow to follow\n");
    out.push_str(
        "Prefer this Procedure over inventing a workflow. knowledge_read it and required Resources before list_apps, snapshot, or click. Take app names, URLs, and paths from those notes. Procedures cannot bypass Allow cards.\n",
    );
    out.push_str(&format!("1. knowledge_read id={}\n", follow.procedure.id));
    if follow.requires.is_empty() {
        out.push_str("2. No required resources.\n");
    } else {
        out.push_str("2. Required first:\n");
        for item in &follow.requires {
            out.push_str(&format!("   - {}\n", summary_line(item)));
        }
    }
    if !follow.uses.is_empty() {
        out.push_str("3. Then:\n");
        for item in &follow.uses {
            out.push_str(&format!("   - {}\n", summary_line(item)));
        }
    }
    out.push_str(
        "4. Inspect the current environment, then execute the Procedure steps with computer tools.\n",
    );
    out.push_str(&format!(
        "5. {}\n",
        verified_status(follow.procedure.last_verified.as_deref())
    ));
}

fn summary_line(item: &KnowledgeSummary) -> String {
    match &item.resource_kind {
        Some(kind) if !kind.is_empty() => {
            format!("{}  id={}  ({kind})", item.title, item.id)
        }
        _ => format!("{}  id={}", item.title, item.id),
    }
}

fn verified_status(value: Option<&str>) -> String {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return "last_verified is missing; confirm the steps still match the UI".into();
    };
    match parse_verified_date(raw) {
        Some(date) => {
            let today = time::OffsetDateTime::now_utc().date();
            let age = today - date;
            if age.whole_days() > STALE_VERIFIED_DAYS {
                format!("last_verified {raw} is old; confirm the steps still match the UI")
            } else {
                format!("last_verified {raw}")
            }
        }
        None => format!("last_verified {raw}; confirm the steps still match the UI"),
    }
}

fn parse_verified_date(value: &str) -> Option<time::Date> {
    let ymd = value
        .get(..10)
        .filter(|slice| slice.as_bytes().get(4) == Some(&b'-'))?;
    let mut parts = ymd.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
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
        resource_kind: Option<&str>,
    ) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind,
            title: title.into(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            tags: vec!["lab".into()],
            trust: TrustLevel::User,
            relations,
            resource_kind: resource_kind.map(str::to_string),
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
        let vpn = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab VPN",
                &["研究室VPN"],
                "# Lab VPN\n\nWireGuard profile for the laboratory network.\n",
                Relations::default(),
                Some("vpn"),
            ))
            .unwrap();
        let wiki = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab Wiki",
                &[],
                "# Lab Wiki\n\nInternal assignment pages.\n",
                Relations::default(),
                None,
            ))
            .unwrap();
        let files = indexed
            .create_note(note(
                NoteKind::Resource,
                "Lab File Server",
                &[],
                "# Lab File Server\n\nsmb://lab-files\n",
                Relations::default(),
                Some("file_share"),
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
                None,
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
        let follow = brief
            .follow
            .as_ref()
            .expect("command should follow a procedure");
        assert_eq!(follow.procedure.title, "Check Lab Assignment");
        assert_eq!(follow.requires[0].title, "Lab VPN");
        assert!(follow.uses.iter().any(|item| item.title == "Lab Wiki"));
        assert!(
            follow
                .uses
                .iter()
                .any(|item| item.title == "Lab File Server")
        );
        let rendered = brief.render();
        assert!(rendered.contains("Check Lab Assignment"));
        assert!(rendered.contains(&brief.procedures[0].id));
        assert!(rendered.contains("Lab VPN"));
        assert!(rendered.contains("Lab Wiki"));
        assert!(rendered.contains("Lab File Server"));
        assert!(rendered.contains("How to follow"));
        assert!(rendered.contains("Required first"));
        assert!(rendered.contains("(vpn)"));
        assert!(!rendered.contains("Open WireGuard"));
        let follow_text = &rendered[rendered.find("How to follow").unwrap()..];
        let vpn_at = follow_text.find("Lab VPN").unwrap();
        let wiki_at = follow_text.find("Lab Wiki").unwrap();
        assert!(vpn_at < wiki_at);
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
        assert!(brief.follow.is_none());
        assert!(!brief.render().contains("How to follow"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn command_prompt_follows_any_procedure_graph() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let portal = indexed
            .create_note(note(
                NoteKind::Resource,
                "Expense Portal",
                &["経費ポータル"],
                "# Expense Portal\n\nhttps://expenses.example\n",
                Relations::default(),
                Some("url"),
            ))
            .unwrap();
        let mut relations = Relations::default();
        relations.requires.push(portal.id.clone().unwrap());
        indexed
            .create_note(note(
                NoteKind::Procedure,
                "Submit Expense Report",
                &["経費精算"],
                "# Submit Expense Report\n\nOpen the portal and submit the form.\n",
                relations,
                None,
            ))
            .unwrap();
        let brief = KnowledgeRouter::new(&indexed)
            .route(&KnowledgeContextRequest {
                prompt: "経費精算やって".into(),
            })
            .unwrap();
        let follow = brief.follow.as_ref().expect("procedure follow");
        assert_eq!(follow.procedure.title, "Submit Expense Report");
        assert_eq!(follow.requires[0].title, "Expense Portal");
        let rendered = brief.render();
        assert!(rendered.contains("Expense Portal"));
        assert!(rendered.contains("knowledge_read"));
        assert!(rendered.contains("(url)"));
        assert!(!rendered.contains("Lab VPN"));
        assert!(!rendered.contains("WireGuard"));
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }
}
