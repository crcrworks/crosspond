use std::collections::HashMap;

use crate::index::SearchHit;
use crate::model::NoteKind;

use super::query::looks_like_command;

const MAX_MERGED: usize = 24;

pub fn merge_hits(prompt: &str, batches: Vec<Vec<SearchHit>>) -> Vec<SearchHit> {
    let command = looks_like_command(prompt);
    let mut best: HashMap<String, SearchHit> = HashMap::new();
    for hits in batches {
        for mut hit in hits {
            if command && hit.kind == NoteKind::Procedure {
                hit.rank = hit.rank.saturating_sub(2);
            }
            best.entry(hit.id.clone())
                .and_modify(|existing| {
                    if hit.rank < existing.rank {
                        *existing = hit.clone();
                    }
                })
                .or_insert(hit);
        }
    }
    let mut hits: Vec<SearchHit> = best.into_values().collect();
    hits.sort_by(|a, b| {
        kind_boost(command, a.kind)
            .cmp(&kind_boost(command, b.kind))
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.title.cmp(&b.title))
    });
    hits.truncate(MAX_MERGED);
    hits
}

fn kind_boost(command: bool, kind: NoteKind) -> i64 {
    if command {
        match kind {
            NoteKind::Procedure => 0,
            NoteKind::Resource => 1,
            NoteKind::Knowledge | NoteKind::Synthesis => 2,
            NoteKind::Activity => 3,
            NoteKind::Source => 4,
        }
    } else {
        match kind {
            NoteKind::Resource | NoteKind::Knowledge => 0,
            NoteKind::Synthesis => 1,
            NoteKind::Procedure => 2,
            NoteKind::Activity => 3,
            NoteKind::Source => 4,
        }
    }
}
