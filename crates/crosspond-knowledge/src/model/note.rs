use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{KnowledgeId, Relations, SourceStatus, TrustLevel};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Knowledge,
    Resource,
    Procedure,
    Source,
    Activity,
    Synthesis,
}

impl NoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Knowledge => "knowledge",
            Self::Resource => "resource",
            Self::Procedure => "procedure",
            Self::Source => "source",
            Self::Activity => "activity",
            Self::Synthesis => "synthesis",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "knowledge" => Ok(Self::Knowledge),
            "resource" => Ok(Self::Resource),
            "procedure" => Ok(Self::Procedure),
            "source" => Ok(Self::Source),
            "activity" => Ok(Self::Activity),
            "synthesis" => Ok(Self::Synthesis),
            other => Err(other.to_string()),
        }
    }

    pub fn default_dir(self) -> &'static str {
        match self {
            Self::Knowledge => "knowledge/entities",
            Self::Resource => "resources",
            Self::Procedure => "procedures",
            Self::Source => "sources",
            Self::Activity => "history",
            Self::Synthesis => "knowledge/syntheses",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiLink {
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeNote {
    pub id: Option<KnowledgeId>,
    pub kind: NoteKind,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub trust: Option<TrustLevel>,
    pub relations: Relations,
    pub sources: Vec<KnowledgeId>,
    pub resource_kind: Option<String>,
    pub credential_ref: Option<String>,
    pub last_verified: Option<String>,
    pub source_kind: Option<String>,
    pub url: Option<String>,
    pub source_status: Option<SourceStatus>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub body: String,
    pub path: PathBuf,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewKnowledgeNote {
    pub kind: NoteKind,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub trust: TrustLevel,
    pub relations: Relations,
    pub resource_kind: Option<String>,
    pub credential_ref: Option<String>,
    pub body: String,
    pub relative_path: Option<PathBuf>,
    pub url: Option<String>,
    pub source_kind: Option<String>,
    pub source_status: Option<SourceStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgePatch {
    pub id: KnowledgeId,
    pub expected_hash: String,
    pub title: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub trust: Option<TrustLevel>,
    pub relations: Option<Relations>,
    pub last_verified: Option<String>,
    pub source_status: Option<SourceStatus>,
    pub body: Option<String>,
}
