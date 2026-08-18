use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use super::VaultError;
use super::hash::content_hash;
use crate::model::{
    KnowledgeId, KnowledgeNote, NoteKind, Relations, SourceStatus, TrustLevel, WikiLink,
};

pub fn parse_markdown(
    vault_root: &Path,
    path: &Path,
    bytes: &[u8],
) -> Result<KnowledgeNote, VaultError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|err| VaultError::Io(err.to_string()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let (frontmatter, body) = split_frontmatter(text)?;
    let relative = path.strip_prefix(vault_root).unwrap_or(path).to_path_buf();
    let hash = content_hash(bytes);
    match frontmatter {
        None => Ok(unmanaged_note(relative, body, hash)),
        Some(yaml) => parse_managed(relative, yaml, body, hash),
    }
}

pub fn parse_wikilinks(body: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut search_from = 0;
    while let Some(start) = body[search_from..].find("[[") {
        let abs = search_from + start;
        let inner_at = abs + 2;
        let Some(end) = body[inner_at..].find("]]") else {
            break;
        };
        let inner = &body[inner_at..inner_at + end];
        search_from = inner_at + end + 2;
        if inner.is_empty() || inner.contains('[') || inner.contains('\n') {
            continue;
        }
        links.push(parse_wikilink_inner(inner));
    }
    links
}

fn parse_wikilink_inner(inner: &str) -> WikiLink {
    let (target_and_heading, alias) = match inner.split_once('|') {
        Some((left, right)) => (left.trim(), Some(right.trim().to_string())),
        None => (inner.trim(), None),
    };
    let (target, heading) = match target_and_heading.split_once('#') {
        Some((target, heading)) => (target.trim(), Some(heading.trim().to_string())),
        None => (target_and_heading, None),
    };
    WikiLink {
        target: target.to_string(),
        heading: heading.filter(|value| !value.is_empty()),
        alias: alias.filter(|value| !value.is_empty()),
    }
}

fn split_frontmatter(text: &str) -> Result<(Option<String>, String), VaultError> {
    if !text.starts_with("---") {
        return Ok((None, text.to_string()));
    }
    let after_open = match text.strip_prefix("---\r\n") {
        Some(rest) => rest,
        None => match text.strip_prefix("---\n") {
            Some(rest) => rest,
            None => return Ok((None, text.to_string())),
        },
    };
    if let Some(body) = after_open.strip_prefix("---\r\n") {
        return Ok((Some(String::new()), body.to_string()));
    }
    if let Some(body) = after_open.strip_prefix("---\n") {
        return Ok((Some(String::new()), body.to_string()));
    }
    let close = after_open
        .find("\n---\r\n")
        .map(|i| (i, 6))
        .or_else(|| after_open.find("\n---\n").map(|i| (i, 5)))
        .or_else(|| after_open.find("\r\n---\r\n").map(|i| (i, 7)));
    let Some((end, sep_len)) = close else {
        return Err(VaultError::MalformedFrontmatter);
    };
    let yaml = after_open[..end].replace('\r', "");
    let body = after_open[end + sep_len..].to_string();
    Ok((Some(yaml), body))
}

#[derive(Debug, Deserialize)]
struct RawFrontmatter {
    id: Option<String>,
    #[serde(rename = "type")]
    note_type: Option<String>,
    title: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    trust: Option<String>,
    #[serde(default)]
    relations: Relations,
    #[serde(default)]
    sources: Vec<String>,
    resource_kind: Option<String>,
    credential_ref: Option<String>,
    last_verified: Option<String>,
    source_kind: Option<String>,
    url: Option<String>,
    status: Option<String>,
    created: Option<String>,
    updated: Option<String>,
}

fn parse_managed(
    relative: PathBuf,
    yaml: String,
    body: String,
    hash: String,
) -> Result<KnowledgeNote, VaultError> {
    if yaml.trim().is_empty() {
        return Ok(unmanaged_note(relative, body, hash));
    }
    let raw: RawFrontmatter =
        serde_yaml::from_str(&yaml).map_err(|_| VaultError::MalformedFrontmatter)?;
    let kind = match raw.note_type {
        Some(ref value) => NoteKind::parse(value).map_err(VaultError::UnknownKind)?,
        None => infer_kind(&relative),
    };
    let id = match raw.id {
        Some(raw_id) => Some(KnowledgeId::from_str(&raw_id).map_err(|_| VaultError::InvalidId)?),
        None => None,
    };
    let title = raw.title.unwrap_or_else(|| title_from_path(&relative));
    Ok(KnowledgeNote {
        id,
        kind,
        title,
        aliases: raw.aliases,
        tags: raw.tags,
        trust: raw.trust.as_deref().and_then(parse_trust),
        relations: raw.relations,
        sources: raw
            .sources
            .into_iter()
            .filter_map(|raw| KnowledgeId::from_str(&raw).ok())
            .collect(),
        resource_kind: raw.resource_kind,
        credential_ref: raw.credential_ref,
        last_verified: raw.last_verified,
        source_kind: raw.source_kind,
        url: raw.url,
        source_status: raw.status.as_deref().and_then(parse_status),
        created: raw.created,
        updated: raw.updated,
        body,
        path: relative,
        content_hash: hash,
    })
}

fn unmanaged_note(relative: PathBuf, body: String, hash: String) -> KnowledgeNote {
    KnowledgeNote {
        id: None,
        kind: infer_kind(&relative),
        title: title_from_path(&relative),
        aliases: Vec::new(),
        tags: Vec::new(),
        trust: None,
        relations: Relations::default(),
        sources: Vec::new(),
        resource_kind: None,
        credential_ref: None,
        last_verified: None,
        source_kind: None,
        url: None,
        source_status: None,
        created: None,
        updated: None,
        body,
        path: relative,
        content_hash: hash,
    }
}

fn infer_kind(relative: &Path) -> NoteKind {
    let text = relative.to_string_lossy();
    if text.starts_with("resources/") {
        NoteKind::Resource
    } else if text.starts_with("procedures/") {
        NoteKind::Procedure
    } else if text.starts_with("sources/") {
        NoteKind::Source
    } else if text.starts_with("history/") {
        NoteKind::Activity
    } else if text.starts_with("knowledge/syntheses/") {
        NoteKind::Synthesis
    } else {
        NoteKind::Knowledge
    }
}

fn title_from_path(relative: &Path) -> String {
    relative
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn parse_trust(raw: &str) -> Option<TrustLevel> {
    match raw {
        "user" => Some(TrustLevel::User),
        "reviewed" => Some(TrustLevel::Reviewed),
        "derived" => Some(TrustLevel::Derived),
        "external" => Some(TrustLevel::External),
        _ => None,
    }
}

fn parse_status(raw: &str) -> Option<SourceStatus> {
    match raw {
        "unread" => Some(SourceStatus::Unread),
        "processing" => Some(SourceStatus::Processing),
        "processed" => Some(SourceStatus::Processed),
        "archived" => Some(SourceStatus::Archived),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(path: &str, text: &str) -> KnowledgeNote {
        parse_markdown(Path::new("/vault"), Path::new(path), text.as_bytes()).unwrap()
    }

    #[test]
    fn parses_resource_frontmatter_and_aliases() {
        let note = parse(
            "/vault/resources/Lab VPN.md",
            concat!(
                "---\n",
                "id: cp_resource_lab_vpn\n",
                "type: resource\n",
                "resource_kind: vpn\n",
                "title: Lab VPN\n",
                "aliases:\n",
                "  - 研究室VPN\n",
                "tags:\n",
                "  - lab\n",
                "trust: user\n",
                "relations:\n",
                "  uses:\n",
                "    - cp_resource_lab_wiki\n",
                "---\n\n",
                "# Lab VPN\n\n",
                "Needs [[Lab Wiki]].\n",
            ),
        );
        assert_eq!(note.id.unwrap().as_str(), "cp_resource_lab_vpn");
        assert_eq!(note.kind, NoteKind::Resource);
        assert_eq!(note.title, "Lab VPN");
        assert_eq!(note.aliases, ["研究室VPN"]);
        assert_eq!(note.tags, ["lab"]);
        assert_eq!(note.resource_kind.as_deref(), Some("vpn"));
        assert_eq!(note.relations.uses[0].as_str(), "cp_resource_lab_wiki");
        let links = parse_wikilinks(&note.body);
        assert_eq!(links[0].target, "Lab Wiki");
    }

    #[test]
    fn missing_frontmatter_uses_filename_title() {
        let note = parse("/vault/resources/Lab Wiki.md", "# Hello\n");
        assert!(note.id.is_none());
        assert_eq!(note.kind, NoteKind::Resource);
        assert_eq!(note.title, "Lab Wiki");
        assert_eq!(note.body, "# Hello\n");
    }

    #[test]
    fn malformed_yaml_is_an_error() {
        let err = parse_markdown(
            Path::new("/vault"),
            Path::new("/vault/resources/x.md"),
            b"---\n[[not yaml\n---\nbody\n",
        )
        .unwrap_err();
        assert_eq!(err, VaultError::MalformedFrontmatter);
    }

    #[test]
    fn unknown_kind_is_an_error() {
        let err = parse_markdown(
            Path::new("/vault"),
            Path::new("/vault/x.md"),
            b"---\nid: cp_1\ntype: recipe\ntitle: X\n---\n\n",
        )
        .unwrap_err();
        assert_eq!(err, VaultError::UnknownKind("recipe".into()));
    }

    #[test]
    fn parses_crlf_frontmatter() {
        let text = "---\r\nid: cp_1\r\ntype: knowledge\r\ntitle: CRLF\r\n---\r\n\r\nBody\r\n";
        let note = parse_markdown(
            Path::new("/vault"),
            Path::new("/vault/knowledge/entities/CRLF.md"),
            text.as_bytes(),
        )
        .unwrap();
        assert_eq!(note.title, "CRLF");
        assert!(note.body.contains("Body"));
    }

    #[test]
    fn wikilinks_support_alias_and_heading() {
        let links = parse_wikilinks("See [[Lab VPN#Profile|VPN]] and [[Index]].");
        assert_eq!(links[0].target, "Lab VPN");
        assert_eq!(links[0].heading.as_deref(), Some("Profile"));
        assert_eq!(links[0].alias.as_deref(), Some("VPN"));
        assert_eq!(links[1].target, "Index");
    }
}
