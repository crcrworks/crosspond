use std::sync::Arc;

use serde_json::{Value, json};

use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult, truncate_output};

pub trait KnowledgeBackend: Send + Sync {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String>;
    fn read(&self, id: &str) -> Result<KnowledgeRecord, String>;
    fn neighbors(&self, id: &str) -> Result<Vec<KnowledgeEdge>, String>;
    fn backlinks(&self, id: &str) -> Result<Vec<KnowledgeEdge>, String>;
    fn find_procedure(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String>;
    fn ingest(
        &self,
        title: &str,
        body: &str,
        url: Option<&str>,
        source_kind: Option<&str>,
    ) -> Result<String, String>;
    fn propose_update(&self, id: &str) -> Result<String, String>;
    fn save_unread(
        &self,
        title: &str,
        body: &str,
        url: Option<&str>,
        source_kind: Option<&str>,
    ) -> Result<String, String>;
    fn archive_source(&self, id: &str) -> Result<String, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeHit {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub snippet: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeRecord {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub body: String,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeEdge {
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub title: Option<String>,
}

const NO_VAULT: &str = "No Knowledge Vault is configured. Set vault_path in Settings.";

pub fn register_knowledge_tools(registry: &mut ToolRegistry) {
    registry.register(Arc::new(KnowledgeSearch));
    registry.register(Arc::new(KnowledgeRead));
    registry.register(Arc::new(KnowledgeNeighbors));
    registry.register(Arc::new(KnowledgeBacklinks));
    registry.register(Arc::new(KnowledgeFindProcedure));
    registry.register(Arc::new(KnowledgeIngest));
    registry.register(Arc::new(KnowledgeProposeUpdate));
    registry.register(Arc::new(KnowledgeReadLater));
    registry.register(Arc::new(KnowledgeArchiveSource));
}

fn backend(context: &ToolContext) -> Result<&dyn KnowledgeBackend, ToolError> {
    context
        .knowledge
        .as_deref()
        .ok_or_else(|| ToolError::Failed(NO_VAULT.into()))
}

fn required_query(input: &Value) -> Result<String, ToolError> {
    input
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed("query is required".into()))
}

fn required_id(input: &Value) -> Result<String, ToolError> {
    input
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed("id is required".into()))
}

fn limit(input: &Value) -> usize {
    input
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 20) as usize
}

fn format_hits(hits: &[KnowledgeHit]) -> String {
    if hits.is_empty() {
        return "No matching notes.".into();
    }
    let mut out = String::new();
    for hit in hits {
        out.push_str(&format!("- {} [{}] id={}\n", hit.title, hit.kind, hit.id));
        if !hit.snippet.is_empty() {
            out.push_str(&format!("  {}\n", hit.snippet));
        }
    }
    truncate_output(out)
}

fn format_edges(edges: &[KnowledgeEdge]) -> String {
    if edges.is_empty() {
        return "No linked notes.".into();
    }
    let mut out = String::new();
    for edge in edges {
        let title = edge.title.as_deref().unwrap_or("(unknown)");
        out.push_str(&format!(
            "- {title} id={} via {}\n",
            edge.target_id, edge.relation
        ));
    }
    truncate_output(out)
}

struct KnowledgeSearch;

impl Tool for KnowledgeSearch {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_search".into(),
            description: "Search the Knowledge Vault by title, alias, tags, or body. Returns short hits, not full notes.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Max hits (1-20)" }
                },
                "required": ["query"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let query = required_query(&input)?;
        let hits = backend(context)?
            .search(&query, limit(&input))
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: format_hits(&hits),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeRead;

impl Tool for KnowledgeRead {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_read".into(),
            description: "Read one Knowledge Vault note by id. Use after knowledge_search.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Note id from search or the brief" }
                },
                "required": ["id"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let id = required_id(&input)?;
        let note = backend(context)?.read(&id).map_err(ToolError::Failed)?;
        let mut text = format!(
            "title: {}\nkind: {}\nid: {}\npath: {}\n",
            note.title, note.kind, note.id, note.path
        );
        if !note.aliases.is_empty() {
            text.push_str(&format!("aliases: {}\n", note.aliases.join(", ")));
        }
        if !note.tags.is_empty() {
            text.push_str(&format!("tags: {}\n", note.tags.join(", ")));
        }
        text.push('\n');
        text.push_str(&note.body);
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeNeighbors;

impl Tool for KnowledgeNeighbors {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_neighbors".into(),
            description:
                "List notes linked from a Knowledge Vault note (wikilinks and typed relations)."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Note id" }
                },
                "required": ["id"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let id = required_id(&input)?;
        let edges = backend(context)?
            .neighbors(&id)
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: format_edges(&edges),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeBacklinks;

impl Tool for KnowledgeBacklinks {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_backlinks".into(),
            description: "List notes that link to a Knowledge Vault note.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Note id" }
                },
                "required": ["id"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let id = required_id(&input)?;
        let edges = backend(context)?
            .backlinks(&id)
            .map_err(ToolError::Failed)?;
        if edges.is_empty() {
            return Ok(ToolResult {
                text: "No linked notes.".into(),
                created_file: None,
                image: None,
            });
        }
        let mut out = String::new();
        for edge in &edges {
            let title = edge.title.as_deref().unwrap_or("(unknown)");
            out.push_str(&format!(
                "- {title} id={} via {}\n",
                edge.source_id, edge.relation
            ));
        }
        Ok(ToolResult {
            text: truncate_output(out),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeFindProcedure;

impl Tool for KnowledgeFindProcedure {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_find_procedure".into(),
            description:
                "Find Procedure notes in the Knowledge Vault for how the user wants a task done."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Task or procedure query" },
                    "limit": { "type": "integer", "description": "Max hits (1-20)" }
                },
                "required": ["query"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let query = required_query(&input)?;
        let hits = backend(context)?
            .find_procedure(&query, limit(&input))
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: format_hits(&hits),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeIngest;

impl Tool for KnowledgeIngest {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_ingest".into(),
            description: "Capture a Source into the Knowledge Vault and apply a validated update plan against existing notes. Do not pass secrets. The vault, not this tool, chooses which notes to create or patch.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Source title" },
                    "body": { "type": "string", "description": "Source text (untrusted)" },
                    "url": { "type": "string", "description": "Optional source URL" },
                    "source_kind": { "type": "string", "description": "url, text, pdf, or file" }
                },
                "required": ["title", "body"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let title = input
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::Failed("title is required".into()))?;
        let body = input
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if body.is_empty() {
            return Err(ToolError::Failed("body is required".into()));
        }
        let url = input.get("url").and_then(Value::as_str);
        let source_kind = input.get("source_kind").and_then(Value::as_str);
        let text = backend(context)?
            .ingest(title, body, url, source_kind)
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeProposeUpdate;

impl Tool for KnowledgeProposeUpdate {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_propose_update".into(),
            description: "Process an existing Source id: apply a validated update plan and mark it processed. Does not accept arbitrary note bodies.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Source note id" }
                },
                "required": ["id"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let id = required_id(&input)?;
        let text = backend(context)?
            .propose_update(&id)
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeReadLater;

impl Tool for KnowledgeReadLater {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_read_later".into(),
            description: "Save a URL, selected text, PDF, or local document as an unread Source. Do not pass secrets. Later, knowledge_propose_update connects it into existing notes.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Source title" },
                    "body": { "type": "string", "description": "Source text (untrusted). For a PDF or binary file, pass the filename only." },
                    "url": { "type": "string", "description": "Optional page URL" },
                    "source_kind": { "type": "string", "description": "url, text, pdf, or file" }
                },
                "required": ["title"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let title = input
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::Failed("title is required".into()))?;
        let body = input
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let url = input.get("url").and_then(Value::as_str);
        let source_kind = input.get("source_kind").and_then(Value::as_str);
        let text = backend(context)?
            .save_unread(title, body, url, source_kind)
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

struct KnowledgeArchiveSource;

impl Tool for KnowledgeArchiveSource {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_archive_source".into(),
            description: "Mark a Source as archived. Does not delete the Markdown file.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Source note id" }
                },
                "required": ["id"]
            }),
        }
    }

    fn execute(&self, context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let id = required_id(&input)?;
        let text = backend(context)?
            .archive_source(&id)
            .map_err(ToolError::Failed)?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolContext;

    struct MockVault;

    impl KnowledgeBackend for MockVault {
        fn search(&self, query: &str, _limit: usize) -> Result<Vec<KnowledgeHit>, String> {
            Ok(vec![KnowledgeHit {
                id: "cp_proc".into(),
                title: "Check Lab Assignment".into(),
                kind: "procedure".into(),
                snippet: query.to_string(),
            }])
        }

        fn read(&self, id: &str) -> Result<KnowledgeRecord, String> {
            Ok(KnowledgeRecord {
                id: id.into(),
                title: "Check Lab Assignment".into(),
                kind: "procedure".into(),
                aliases: vec!["研究室の課題確認".into()],
                tags: vec!["lab".into()],
                body: "Enable VPN first.\n".into(),
                path: "procedures/Check Lab Assignment.md".into(),
            })
        }

        fn neighbors(&self, _id: &str) -> Result<Vec<KnowledgeEdge>, String> {
            Ok(vec![KnowledgeEdge {
                source_id: "cp_proc".into(),
                target_id: "cp_vpn".into(),
                relation: "requires".into(),
                title: Some("Lab VPN".into()),
            }])
        }

        fn backlinks(&self, _id: &str) -> Result<Vec<KnowledgeEdge>, String> {
            Ok(Vec::new())
        }

        fn find_procedure(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String> {
            self.search(query, limit)
        }

        fn ingest(
            &self,
            title: &str,
            _body: &str,
            _url: Option<&str>,
            _source_kind: Option<&str>,
        ) -> Result<String, String> {
            Ok(format!(
                "SOURCE:\n{title}\n\nUPDATE:\n- Summer Assignment\n"
            ))
        }

        fn propose_update(&self, id: &str) -> Result<String, String> {
            Ok(format!("SOURCE:\n{id}\n"))
        }

        fn save_unread(
            &self,
            title: &str,
            _body: &str,
            _url: Option<&str>,
            _source_kind: Option<&str>,
        ) -> Result<String, String> {
            Ok(format!("Saved unread source: {title}"))
        }

        fn archive_source(&self, id: &str) -> Result<String, String> {
            Ok(format!("Archived source {id}"))
        }
    }

    fn ctx() -> ToolContext {
        let mut context = ToolContext::new();
        context.knowledge = Some(Arc::new(MockVault));
        context
    }

    #[test]
    fn search_returns_hits() {
        let result = KnowledgeSearch
            .execute(&ctx(), json!({ "query": "課題" }))
            .unwrap();
        assert!(result.text.contains("Check Lab Assignment"));
        assert!(!result.text.contains("Enable VPN first"));
    }

    #[test]
    fn read_returns_body_without_logging_requirement() {
        let result = KnowledgeRead
            .execute(&ctx(), json!({ "id": "cp_proc" }))
            .unwrap();
        assert!(result.text.contains("Enable VPN first"));
        assert!(result.text.contains("研究室の課題確認"));
    }

    #[test]
    fn missing_vault_is_a_clear_error() {
        let err = KnowledgeSearch
            .execute(&ToolContext::new(), json!({ "query": "lab" }))
            .unwrap_err();
        assert!(err.to_string().contains("vault_path"));
    }

    #[test]
    fn neighbors_and_find_procedure_are_registered() {
        let result = KnowledgeNeighbors
            .execute(&ctx(), json!({ "id": "cp_proc" }))
            .unwrap();
        assert!(result.text.contains("Lab VPN"));
        assert!(result.text.contains("cp_vpn"));
        let found = KnowledgeFindProcedure
            .execute(&ctx(), json!({ "query": "研究室の課題確認して" }))
            .unwrap();
        assert!(found.text.contains("Check Lab Assignment"));
        let mut registry = ToolRegistry::new();
        register_knowledge_tools(&mut registry);
        assert!(registry.get("knowledge_search").is_some());
        assert!(registry.get("knowledge_find_procedure").is_some());
        assert!(registry.get("knowledge_ingest").is_some());
        assert!(registry.get("knowledge_propose_update").is_some());
        assert!(registry.get("knowledge_read_later").is_some());
        assert!(registry.get("knowledge_archive_source").is_some());
        let ingested = KnowledgeIngest
            .execute(
                &ctx(),
                json!({ "title": "New Laboratory Assignment", "body": "See Summer Assignment" }),
            )
            .unwrap();
        assert!(ingested.text.contains("Summer Assignment"));
        let saved = KnowledgeReadLater
            .execute(
                &ctx(),
                json!({ "title": "Safari page", "url": "https://example.invalid", "source_kind": "url" }),
            )
            .unwrap();
        assert!(saved.text.contains("unread"));
    }
}
