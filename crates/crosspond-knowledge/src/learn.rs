use crate::index::IndexedVault;
use crate::ingest::looks_like_secret;
use crate::model::{KnowledgeId, KnowledgeNote, NewKnowledgeNote, NoteKind, Relations, TrustLevel};
use crate::retrieval::{looks_like_command, search_queries};
use crate::vault::{VaultError, VaultRepository, format_wikilink, format_wikilink_for_title};

const TITLE_CHARS: usize = 48;
const MAX_STEPS: usize = 12;
const MIN_ACTIONS: usize = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedResource {
    pub id: KnowledgeId,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnRequest {
    pub prompt: String,
    pub actions: Vec<String>,
    pub followed_procedure: bool,
    /// User asked to save via `@vault-procedure`. Skips command/question heuristics.
    pub explicit: bool,
    pub resources: Vec<LinkedResource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcedureProposal {
    pub title: String,
    pub aliases: Vec<String>,
    pub body: String,
    pub steps: Vec<String>,
    pub uses: Vec<KnowledgeId>,
    pub resource_titles: Vec<String>,
}

impl ProcedureProposal {
    pub fn render(&self) -> String {
        let mut out = format!("{}\n", self.title);
        out.push_str(
            "\nThis run will be saved as a Procedure. Crosspond will follow it next time.\n",
        );
        if !self.steps.is_empty() {
            out.push_str("\nSteps:\n");
            for step in &self.steps {
                out.push_str(&format!("- {step}\n"));
            }
        }
        if !self.resource_titles.is_empty() {
            out.push_str("\nResources:\n");
            for title in &self.resource_titles {
                out.push_str(&format!("- {}\n", format_wikilink_for_title(title)));
            }
        }
        out
    }
}

pub struct ProcedureLearner<'a> {
    vault: &'a IndexedVault,
}

impl<'a> ProcedureLearner<'a> {
    pub fn new(vault: &'a IndexedVault) -> Self {
        Self { vault }
    }

    pub fn propose(&self, request: &LearnRequest) -> Result<Option<ProcedureProposal>, VaultError> {
        if request.actions.len() < MIN_ACTIONS {
            return Ok(None);
        }
        if !request.explicit {
            if request.followed_procedure {
                return Ok(None);
            }
            if !looks_like_command(&request.prompt) || looks_like_question(&request.prompt) {
                return Ok(None);
            }
        }
        if looks_like_secret(&request.prompt)
            || request
                .actions
                .iter()
                .any(|action| looks_like_secret(action))
        {
            return Ok(None);
        }
        let title = procedure_title(&request.prompt);
        if title.is_empty() {
            return Ok(None);
        }
        if self.existing_procedure(&title)? {
            return Ok(None);
        }
        let aliases = procedure_aliases(&request.prompt, &title);
        let steps: Vec<String> = request
            .actions
            .iter()
            .map(|action| action.trim().to_string())
            .filter(|action| !action.is_empty())
            .take(MAX_STEPS)
            .collect();
        let uses: Vec<KnowledgeId> = request
            .resources
            .iter()
            .map(|resource| resource.id.clone())
            .collect();
        let resource_titles: Vec<String> = request
            .resources
            .iter()
            .map(|resource| resource.title.clone())
            .collect();
        let body = render_body(&title, &steps, &request.resources, self.vault);
        Ok(Some(ProcedureProposal {
            title,
            aliases,
            body,
            steps,
            uses,
            resource_titles,
        }))
    }

    pub fn save(&self, proposal: &ProcedureProposal) -> Result<KnowledgeNote, VaultError> {
        let relations = Relations {
            uses: proposal.uses.clone(),
            ..Relations::default()
        };
        self.vault.create_note(NewKnowledgeNote {
            kind: NoteKind::Procedure,
            title: proposal.title.clone(),
            aliases: proposal.aliases.clone(),
            tags: Vec::new(),
            trust: TrustLevel::User,
            relations,
            resource_kind: None,
            credential_ref: None,
            body: proposal.body.clone(),
            relative_path: None,
            url: None,
            source_kind: None,
            source_status: None,
        })
    }

    fn existing_procedure(&self, title: &str) -> Result<bool, VaultError> {
        for hit in self.vault.find_procedure(title, 8)? {
            if hit.title == title {
                return Ok(true);
            }
        }
        let expected = crate::vault::default_relative_path(
            NoteKind::Procedure,
            title,
            &time::OffsetDateTime::now_utc(),
        )?;
        Ok(self.vault.repository().root().join(expected).exists())
    }
}

fn looks_like_question(prompt: &str) -> bool {
    let text = prompt.trim();
    text.ends_with('?') || text.ends_with('？') || text.contains("って何") || text.contains("とは")
}

fn procedure_title(prompt: &str) -> String {
    let queries = search_queries(prompt);
    let base = queries
        .get(1)
        .cloned()
        .or_else(|| queries.first().cloned())
        .unwrap_or_default();
    truncate(&base, TITLE_CHARS)
}

fn procedure_aliases(prompt: &str, title: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let stripped = procedure_title(prompt);
    if stripped != title && !stripped.is_empty() {
        aliases.push(stripped);
    }
    let original = prompt.trim();
    if original != title && !original.is_empty() && original.chars().count() <= TITLE_CHARS {
        aliases.push(original.to_string());
    }
    aliases.retain(|alias| alias != title);
    aliases.sort();
    aliases.dedup();
    aliases
}

fn render_body(
    title: &str,
    actions: &[String],
    resources: &[LinkedResource],
    vault: &IndexedVault,
) -> String {
    let mut body = format!(
        "# {title}\n\nTaught from a successful Crosspond run. Guidance only; it cannot bypass Allow.\n\n## Steps\n\n"
    );
    for action in actions.iter().take(MAX_STEPS) {
        let line = action.trim();
        if line.is_empty() {
            continue;
        }
        body.push_str(&format!("- {line}\n"));
    }
    if !resources.is_empty() {
        body.push_str("\n## Resources\n\n");
        for resource in resources {
            let link = match vault.index().lookup(resource.id.as_str()) {
                Ok(Some(hit)) => format_wikilink(&hit.title, &hit.path),
                _ => format_wikilink_for_title(&resource.title),
            };
            body.push_str(&format!("- {link}\n"));
        }
    }
    body
}

fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    let mut out: String = trimmed.chars().take(max).collect();
    if trimmed.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexedVault;
    use crate::retrieval::{KnowledgeContextRequest, KnowledgeRouter};
    use std::fs;

    fn temp_paths() -> (std::path::PathBuf, std::path::PathBuf) {
        let id = uuid::Uuid::now_v7();
        (
            std::env::temp_dir().join(format!("crosspond-learn-vault-{id}")),
            std::env::temp_dir().join(format!("crosspond-learn-db-{id}.sqlite")),
        )
    }

    fn note(kind: NoteKind, title: &str, aliases: &[&str], body: &str) -> NewKnowledgeNote {
        NewKnowledgeNote {
            kind,
            title: title.into(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            tags: Vec::new(),
            trust: TrustLevel::User,
            relations: Relations::default(),
            resource_kind: None,
            credential_ref: None,
            body: body.into(),
            relative_path: None,
            url: None,
            source_kind: None,
            source_status: None,
        }
    }

    #[test]
    fn guided_run_saves_a_procedure_that_later_routes() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let portal = indexed
            .create_note(note(
                NoteKind::Resource,
                "Expense Portal",
                &[],
                "# Expense Portal\n\nhttps://expense.example.invalid\n",
            ))
            .unwrap();
        let learner = ProcedureLearner::new(&indexed);
        let proposal = learner
            .propose(&LearnRequest {
                prompt: "経費精算して".into(),
                actions: vec!["Opened Expense Portal".into(), "Clicked Submit".into()],
                followed_procedure: false,
                explicit: false,
                resources: vec![LinkedResource {
                    id: portal.id.clone().unwrap(),
                    title: "Expense Portal".into(),
                }],
            })
            .unwrap()
            .expect("proposal");
        assert_eq!(proposal.title, "経費精算");
        assert!(proposal.render().contains("経費精算"));
        assert!(proposal.render().contains("Opened Expense Portal"));
        assert!(proposal.render().contains("[[Expense Portal]]"));
        let written = learner.save(&proposal).unwrap();
        assert_eq!(written.kind, NoteKind::Procedure);
        assert!(written.body.contains("Opened Expense Portal"));
        assert!(written.body.contains("[[Expense Portal]]"));
        assert!(written.relations.uses.contains(&portal.id.clone().unwrap()));
        let hits = indexed.find_procedure("経費精算して", 4).unwrap();
        assert!(hits.iter().any(|hit| hit.title == "経費精算"));
        let brief = KnowledgeRouter::new(&indexed)
            .route(&KnowledgeContextRequest::new("経費精算して"))
            .unwrap();
        assert_eq!(
            brief
                .follow
                .as_ref()
                .map(|follow| follow.procedure.title.as_str()),
            Some("経費精算")
        );
        assert!(
            brief
                .follow
                .as_ref()
                .unwrap()
                .uses
                .iter()
                .any(|item| item.title == "Expense Portal")
        );
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn questions_existing_follow_and_secrets_are_not_saved() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let learner = ProcedureLearner::new(&indexed);
        let actions = vec!["Opened app".into(), "Clicked Submit".into()];
        assert!(
            learner
                .propose(&LearnRequest {
                    prompt: "経費精算って何".into(),
                    actions: actions.clone(),
                    followed_procedure: false,
                    explicit: false,
                    resources: Vec::new(),
                })
                .unwrap()
                .is_none()
        );
        assert!(
            learner
                .propose(&LearnRequest {
                    prompt: "経費精算して".into(),
                    actions: actions.clone(),
                    followed_procedure: true,
                    explicit: false,
                    resources: Vec::new(),
                })
                .unwrap()
                .is_none()
        );
        assert!(
            learner
                .propose(&LearnRequest {
                    prompt: "経費精算して".into(),
                    actions: vec!["Opened app".into()],
                    followed_procedure: false,
                    explicit: false,
                    resources: Vec::new(),
                })
                .unwrap()
                .is_none()
        );
        assert!(
            learner
                .propose(&LearnRequest {
                    prompt: "経費精算して".into(),
                    actions: vec!["api_key=sk-test".into(), "Clicked Submit".into()],
                    followed_procedure: false,
                    explicit: false,
                    resources: Vec::new(),
                })
                .unwrap()
                .is_none()
        );
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }

    #[test]
    fn explicit_request_saves_a_question_shaped_prompt() {
        let (vault, sqlite) = temp_paths();
        let indexed = IndexedVault::open(&vault, &sqlite).unwrap();
        let learner = ProcedureLearner::new(&indexed);
        let proposal = learner
            .propose(&LearnRequest {
                prompt: "経費精算って何".into(),
                actions: vec!["Opened app".into(), "Clicked Submit".into()],
                followed_procedure: true,
                explicit: true,
                resources: Vec::new(),
            })
            .unwrap()
            .expect("explicit proposal");
        assert_eq!(proposal.title, "経費精算");
        let _ = fs::remove_dir_all(vault);
        let _ = fs::remove_file(sqlite);
    }
}
