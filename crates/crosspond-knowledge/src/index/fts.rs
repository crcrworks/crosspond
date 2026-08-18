use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{Connection, params};

use super::sqlite::{hit_from_row, index_err};
use crate::model::NoteKind;
use crate::vault::VaultError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub kind: NoteKind,
    pub path: PathBuf,
    pub rank: i64,
}

pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchHit>, VaultError> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut best: HashMap<String, SearchHit> = HashMap::new();
    collect_exact_title(conn, query, &mut best)?;
    collect_exact_alias(conn, query, &mut best)?;
    collect_fts(conn, query, &mut best)?;
    collect_like(conn, query, &mut best)?;
    let mut hits: Vec<SearchHit> = best.into_values().collect();
    hits.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit.min(100));
    Ok(hits)
}

fn collect_exact_title(
    conn: &Connection,
    query: &str,
    best: &mut HashMap<String, SearchHit>,
) -> Result<(), VaultError> {
    let mut stmt = conn
        .prepare("SELECT id, title, kind, path FROM notes WHERE lower(title) = lower(?1)")
        .map_err(index_err)?;
    let rows = stmt
        .query_map(params![query], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(index_err)?;
    for row in rows {
        let (id, title, kind, path) = row.map_err(index_err)?;
        consider(best, id, title, kind, path, 0);
    }
    Ok(())
}

fn collect_exact_alias(
    conn: &Connection,
    query: &str,
    best: &mut HashMap<String, SearchHit>,
) -> Result<(), VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT notes.id, notes.title, notes.kind, notes.path
             FROM aliases
             JOIN notes ON notes.id = aliases.note_id
             WHERE lower(aliases.alias) = lower(?1)",
        )
        .map_err(index_err)?;
    let rows = stmt
        .query_map(params![query], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(index_err)?;
    for row in rows {
        let (id, title, kind, path) = row.map_err(index_err)?;
        consider(best, id, title, kind, path, 1);
    }
    Ok(())
}

fn collect_fts(
    conn: &Connection,
    query: &str,
    best: &mut HashMap<String, SearchHit>,
) -> Result<(), VaultError> {
    let phrase = fts_phrase(query);
    if phrase == "\"\"" {
        return Ok(());
    }
    let mut stmt = match conn.prepare(
        "SELECT notes.id, notes.title, notes.kind, notes.path
         FROM notes_fts
         JOIN notes ON notes.id = notes_fts.id
         WHERE notes_fts MATCH ?1",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Ok(()),
    };
    let rows = match stmt.query_map(params![phrase], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    }) {
        Ok(rows) => rows,
        Err(_) => return Ok(()),
    };
    for row in rows {
        let Ok((id, title, kind, path)) = row else {
            continue;
        };
        consider(best, id, title, kind, path, 2);
    }
    Ok(())
}

fn collect_like(
    conn: &Connection,
    query: &str,
    best: &mut HashMap<String, SearchHit>,
) -> Result<(), VaultError> {
    let pattern = like_pattern(query);
    let mut stmt = conn
        .prepare(
            "SELECT id, title, kind, path FROM notes
             WHERE lower(title) LIKE lower(?1) ESCAPE '\\'
                OR lower(aliases) LIKE lower(?1) ESCAPE '\\'
                OR lower(tags) LIKE lower(?1) ESCAPE '\\'
                OR lower(body) LIKE lower(?1) ESCAPE '\\'
             UNION
             SELECT notes.id, notes.title, notes.kind, notes.path
             FROM aliases
             JOIN notes ON notes.id = aliases.note_id
             WHERE lower(aliases.alias) LIKE lower(?1) ESCAPE '\\'",
        )
        .map_err(index_err)?;
    let rows = stmt
        .query_map(params![pattern], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(index_err)?;
    for row in rows {
        let (id, title, kind, path) = row.map_err(index_err)?;
        consider(best, id, title, kind, path, 3);
    }
    Ok(())
}

fn consider(
    best: &mut HashMap<String, SearchHit>,
    id: String,
    title: String,
    kind: String,
    path: String,
    rank: i64,
) {
    let Some(hit) = hit_from_row(id.clone(), title, kind, path, rank) else {
        return;
    };
    best.entry(id)
        .and_modify(|existing| {
            if hit.rank < existing.rank {
                *existing = hit.clone();
            }
        })
        .or_insert(hit);
}

fn fts_phrase(query: &str) -> String {
    let cleaned: String = query
        .chars()
        .filter(|ch| *ch != '"' && *ch != '\0')
        .collect();
    format!("\"{}\"", cleaned.trim())
}

fn like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}
