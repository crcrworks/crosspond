use serde::{Deserialize, Serialize};

use super::KnowledgeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Related,
    Requires,
    Uses,
    ProducedBy,
    DerivedFrom,
    Mentions,
    Supersedes,
}

impl RelationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Related => "related",
            Self::Requires => "requires",
            Self::Uses => "uses",
            Self::ProducedBy => "produced_by",
            Self::DerivedFrom => "derived_from",
            Self::Mentions => "mentions",
            Self::Supersedes => "supersedes",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "related" => Some(Self::Related),
            "requires" => Some(Self::Requires),
            "uses" => Some(Self::Uses),
            "produced_by" => Some(Self::ProducedBy),
            "derived_from" => Some(Self::DerivedFrom),
            "mentions" => Some(Self::Mentions),
            "supersedes" => Some(Self::Supersedes),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Relations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<KnowledgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<KnowledgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<KnowledgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub produced_by: Vec<KnowledgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<KnowledgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<KnowledgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<KnowledgeId>,
}

impl Relations {
    pub fn is_empty(&self) -> bool {
        self.related.is_empty()
            && self.requires.is_empty()
            && self.uses.is_empty()
            && self.produced_by.is_empty()
            && self.derived_from.is_empty()
            && self.mentions.is_empty()
            && self.supersedes.is_empty()
    }

    pub fn ids_for(&self, kind: RelationKind) -> &[KnowledgeId] {
        match kind {
            RelationKind::Related => &self.related,
            RelationKind::Requires => &self.requires,
            RelationKind::Uses => &self.uses,
            RelationKind::ProducedBy => &self.produced_by,
            RelationKind::DerivedFrom => &self.derived_from,
            RelationKind::Mentions => &self.mentions,
            RelationKind::Supersedes => &self.supersedes,
        }
    }

    pub fn ids_for_mut(&mut self, kind: RelationKind) -> &mut Vec<KnowledgeId> {
        match kind {
            RelationKind::Related => &mut self.related,
            RelationKind::Requires => &mut self.requires,
            RelationKind::Uses => &mut self.uses,
            RelationKind::ProducedBy => &mut self.produced_by,
            RelationKind::DerivedFrom => &mut self.derived_from,
            RelationKind::Mentions => &mut self.mentions,
            RelationKind::Supersedes => &mut self.supersedes,
        }
    }
}
