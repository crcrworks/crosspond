use rusqlite::{Connection, params};

use super::sqlite::index_err;
use crate::vault::VaultError;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct IndexedLink {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
}

pub fn neighbors(conn: &Connection, id: &str) -> Result<Vec<IndexedLink>, VaultError> {
    query_links(
        conn,
        "SELECT source_id, target_id, relation_type FROM links WHERE source_id = ?1 ORDER BY relation_type, target_id",
        id,
    )
}

pub fn backlinks(conn: &Connection, id: &str) -> Result<Vec<IndexedLink>, VaultError> {
    query_links(
        conn,
        "SELECT source_id, target_id, relation_type FROM links WHERE target_id = ?1 ORDER BY relation_type, source_id",
        id,
    )
}

pub fn all_links(conn: &Connection) -> Result<Vec<IndexedLink>, VaultError> {
    let mut stmt = conn
        .prepare("SELECT source_id, target_id, relation_type FROM links")
        .map_err(index_err)?;
    let mapped = stmt.query_map([], map_link).map_err(index_err)?;
    let mut links = Vec::new();
    for row in mapped {
        links.push(row.map_err(index_err)?);
    }
    Ok(links)
}

fn query_links(conn: &Connection, sql: &str, id: &str) -> Result<Vec<IndexedLink>, VaultError> {
    let mut stmt = conn.prepare(sql).map_err(index_err)?;
    let mapped = if sql.contains("?1") {
        stmt.query_map(params![id], map_link).map_err(index_err)?
    } else {
        stmt.query_map([], map_link).map_err(index_err)?
    };
    let mut links = Vec::new();
    for row in mapped {
        links.push(row.map_err(index_err)?);
    }
    Ok(links)
}

fn map_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedLink> {
    Ok(IndexedLink {
        source_id: row.get(0)?,
        target_id: row.get(1)?,
        relation_type: row.get(2)?,
    })
}
