use std::collections::HashSet;
use std::sync::Arc;

use crosspond_knowledge::{IndexedVault, index_note_id, search_queries};
use crosspond_tools::{KnowledgeBackend, KnowledgeEdge, KnowledgeHit, KnowledgeRecord};

pub(crate) struct VaultKnowledge(pub Arc<IndexedVault>);

impl KnowledgeBackend for VaultKnowledge {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String> {
        collect_hits(&self.0, query, limit, |vault, q, n| vault.search(q, n))
    }

    fn read(&self, id: &str) -> Result<KnowledgeRecord, String> {
        let note = self.0.read_indexed(id).map_err(|err| err.to_string())?;
        Ok(KnowledgeRecord {
            id: index_note_id(&note),
            title: note.title,
            kind: note.kind.as_str().into(),
            aliases: note.aliases,
            tags: note.tags,
            body: note.body,
            path: note.path.display().to_string(),
        })
    }

    fn neighbors(&self, id: &str) -> Result<Vec<KnowledgeEdge>, String> {
        map_edges(
            &self.0,
            self.0
                .index()
                .neighbors(id)
                .map_err(|err| err.to_string())?,
            false,
        )
    }

    fn backlinks(&self, id: &str) -> Result<Vec<KnowledgeEdge>, String> {
        map_edges(
            &self.0,
            self.0
                .index()
                .backlinks(id)
                .map_err(|err| err.to_string())?,
            true,
        )
    }

    fn find_procedure(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>, String> {
        collect_hits(&self.0, query, limit, |vault, q, n| {
            vault.find_procedure(q, n)
        })
    }
}

fn collect_hits(
    vault: &IndexedVault,
    query: &str,
    limit: usize,
    search: impl Fn(
        &IndexedVault,
        &str,
        usize,
    ) -> Result<Vec<crosspond_knowledge::SearchHit>, crosspond_knowledge::VaultError>,
) -> Result<Vec<KnowledgeHit>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    // Prefer stripped forms ("研究室の課題確認して" → "研究室の課題確認") so alias hits win.
    for q in search_queries(query).into_iter().rev() {
        for hit in search(vault, &q, limit).map_err(|err| err.to_string())? {
            if seen.insert(hit.id.clone()) {
                out.push(to_hit(vault, hit));
            }
        }
    }
    out.truncate(limit);
    Ok(out)
}

fn to_hit(vault: &IndexedVault, hit: crosspond_knowledge::SearchHit) -> KnowledgeHit {
    let snippet = vault
        .read_indexed(&hit.id)
        .ok()
        .map(|note| first_line(&note.body))
        .unwrap_or_default();
    KnowledgeHit {
        id: hit.id,
        title: hit.title,
        kind: hit.kind.as_str().into(),
        snippet,
    }
}

fn map_edges(
    vault: &IndexedVault,
    links: Vec<crosspond_knowledge::IndexedLink>,
    backlinks: bool,
) -> Result<Vec<KnowledgeEdge>, String> {
    let mut edges = Vec::new();
    for link in links {
        let title_id = if backlinks {
            &link.source_id
        } else {
            &link.target_id
        };
        let title = vault
            .index()
            .lookup(title_id)
            .ok()
            .flatten()
            .map(|hit| hit.title);
        edges.push(KnowledgeEdge {
            source_id: link.source_id,
            target_id: link.target_id,
            relation: link.relation_type,
            title,
        });
    }
    Ok(edges)
}

fn first_line(body: &str) -> String {
    body.lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}
