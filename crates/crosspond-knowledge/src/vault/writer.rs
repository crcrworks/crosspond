use serde_yaml::{Mapping, Value};

use crate::model::{KnowledgeNote, NoteKind};

pub fn render_note(note: &KnowledgeNote) -> String {
    let yaml =
        serde_yaml::to_string(&frontmatter_value(note)).unwrap_or_else(|_| "id: unknown\n".into());
    let yaml = yaml.strip_prefix("---\n").unwrap_or(&yaml);
    let yaml = yaml.strip_suffix('\n').unwrap_or(yaml);
    let body = if note.body.starts_with('\n') {
        note.body.clone()
    } else {
        format!("\n{}", note.body)
    };
    format!("---\n{yaml}\n---\n{body}")
}

fn frontmatter_value(note: &KnowledgeNote) -> Mapping {
    let mut map = Mapping::new();
    if let Some(id) = &note.id {
        insert_string(&mut map, "id", id.as_str());
    }
    insert_string(&mut map, "type", note.kind.as_str());
    insert_string(&mut map, "title", &note.title);
    if note.kind == NoteKind::Resource
        && let Some(kind) = &note.resource_kind
    {
        insert_string(&mut map, "resource_kind", kind);
    }
    if !note.aliases.is_empty() {
        insert_seq(&mut map, "aliases", &note.aliases);
    }
    if !note.tags.is_empty() {
        insert_seq(&mut map, "tags", &note.tags);
    }
    if let Some(trust) = note.trust {
        insert_string(&mut map, "trust", trust.as_str());
    }
    if !note.relations.is_empty() {
        map.insert(
            Value::String("relations".into()),
            Value::Mapping(relations_mapping(note)),
        );
    }
    if !note.sources.is_empty() {
        insert_seq(
            &mut map,
            "sources",
            &note
                .sources
                .iter()
                .map(|id| id.as_str().to_string())
                .collect::<Vec<_>>(),
        );
    }
    if let Some(value) = &note.credential_ref {
        insert_string(&mut map, "credential_ref", value);
    }
    if let Some(value) = &note.last_verified {
        insert_string(&mut map, "last_verified", value);
    }
    if let Some(value) = &note.source_kind {
        insert_string(&mut map, "source_kind", value);
    }
    if let Some(value) = &note.url {
        insert_string(&mut map, "url", value);
    }
    if let Some(status) = note.source_status {
        insert_string(
            &mut map,
            "status",
            match status {
                crate::model::SourceStatus::Unread => "unread",
                crate::model::SourceStatus::Processing => "processing",
                crate::model::SourceStatus::Processed => "processed",
                crate::model::SourceStatus::Archived => "archived",
            },
        );
    }
    if let Some(value) = &note.created {
        insert_string(&mut map, "created", value);
    }
    if let Some(value) = &note.updated {
        insert_string(&mut map, "updated", value);
    }
    map
}

fn relations_mapping(note: &KnowledgeNote) -> Mapping {
    let mut map = Mapping::new();
    for kind in [
        crate::model::RelationKind::Related,
        crate::model::RelationKind::Requires,
        crate::model::RelationKind::Uses,
        crate::model::RelationKind::ProducedBy,
        crate::model::RelationKind::DerivedFrom,
        crate::model::RelationKind::Mentions,
        crate::model::RelationKind::Supersedes,
    ] {
        let ids = note.relations.ids_for(kind);
        if ids.is_empty() {
            continue;
        }
        insert_seq(
            &mut map,
            kind.as_str(),
            &ids.iter()
                .map(|id| id.as_str().to_string())
                .collect::<Vec<_>>(),
        );
    }
    map
}

fn insert_string(map: &mut Mapping, key: &str, value: &str) {
    map.insert(Value::String(key.into()), Value::String(value.to_string()));
}

fn insert_seq(map: &mut Mapping, key: &str, values: &[String]) {
    map.insert(
        Value::String(key.into()),
        Value::Sequence(values.iter().cloned().map(Value::String).collect()),
    );
}
