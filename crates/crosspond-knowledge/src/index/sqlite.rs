use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::graph::IndexedLink;
use super::{SCHEMA_VERSION, SearchHit, SearchIndex, index_note_id, path_str};
use crate::model::{KnowledgeNote, NoteKind, RelationKind};
use crate::vault::{VaultError, parse_wikilinks};

const CREATE_SCHEMA: &str = "
CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    kind TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    aliases TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT ''
);
CREATE TABLE aliases (
    note_id TEXT NOT NULL,
    alias TEXT NOT NULL
);
CREATE TABLE links (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relation_type TEXT NOT NULL
);
CREATE VIRTUAL TABLE notes_fts USING fts5(
    id UNINDEXED,
    title,
    aliases,
    tags,
    body
);
CREATE INDEX idx_aliases_note ON aliases(note_id);
CREATE INDEX idx_aliases_alias ON aliases(alias);
CREATE INDEX idx_links_source ON links(source_id);
CREATE INDEX idx_links_target ON links(target_id);
CREATE UNIQUE INDEX idx_links_unique ON links(source_id, target_id, relation_type);
";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexSnapshot {
    pub notes: Vec<SnapshotNote>,
    pub aliases: Vec<(String, String)>,
    pub links: Vec<IndexedLink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotNote {
    pub id: String,
    pub path: String,
    pub title: String,
    pub kind: String,
    pub content_hash: String,
}

impl SearchIndex {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, VaultError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| VaultError::Io(err.to_string()))?;
        }
        let conn = Connection::open(&path).map_err(index_err)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(index_err)?;
        ensure_schema(&conn)?;
        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn rebuild(&self, notes: &[KnowledgeNote]) -> Result<(), VaultError> {
        let mut conn = lock(&self.conn)?;
        let tx = conn.transaction().map_err(index_err)?;
        tx.execute_batch(
            "DELETE FROM notes_fts;
             DELETE FROM links;
             DELETE FROM aliases;
             DELETE FROM notes;",
        )
        .map_err(index_err)?;
        for note in notes {
            insert_note(&tx, note)?;
        }
        refresh_fts(&tx)?;
        tx.commit().map_err(index_err)
    }

    pub fn upsert_note(&self, note: &KnowledgeNote) -> Result<(), VaultError> {
        let mut conn = lock(&self.conn)?;
        let tx = conn.transaction().map_err(index_err)?;
        upsert_note_tx(&tx, note)?;
        refresh_fts(&tx)?;
        tx.commit().map_err(index_err)
    }

    pub fn remove_path(&self, relative: &Path) -> Result<(), VaultError> {
        let mut conn = lock(&self.conn)?;
        let tx = conn.transaction().map_err(index_err)?;
        remove_path_tx(&tx, relative)?;
        refresh_fts(&tx)?;
        tx.commit().map_err(index_err)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, VaultError> {
        let conn = lock(&self.conn)?;
        super::fts::search(&conn, query, limit)
    }

    pub fn neighbors(&self, id: &str) -> Result<Vec<IndexedLink>, VaultError> {
        let conn = lock(&self.conn)?;
        super::graph::neighbors(&conn, id)
    }

    pub fn backlinks(&self, id: &str) -> Result<Vec<IndexedLink>, VaultError> {
        let conn = lock(&self.conn)?;
        super::graph::backlinks(&conn, id)
    }

    pub fn lookup(&self, id: &str) -> Result<Option<SearchHit>, VaultError> {
        let conn = lock(&self.conn)?;
        let mut stmt = conn
            .prepare("SELECT id, title, kind, path FROM notes WHERE id = ?1")
            .map_err(index_err)?;
        let mut rows = stmt.query(params![id]).map_err(index_err)?;
        match rows.next().map_err(index_err)? {
            Some(row) => Ok(hit_from_row(
                row.get(0).map_err(index_err)?,
                row.get(1).map_err(index_err)?,
                row.get(2).map_err(index_err)?,
                row.get(3).map_err(index_err)?,
                0,
            )),
            None => Ok(None),
        }
    }

    pub fn recent_kind(
        &self,
        kind: crate::model::NoteKind,
        limit: usize,
    ) -> Result<Vec<SearchHit>, VaultError> {
        let conn = lock(&self.conn)?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, kind, path FROM notes
                 WHERE kind = ?1
                 ORDER BY mtime DESC
                 LIMIT ?2",
            )
            .map_err(index_err)?;
        let rows = stmt
            .query_map(params![kind.as_str(), limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(index_err)?;
        let mut hits = Vec::new();
        for row in rows {
            let (id, title, kind, path) = row.map_err(index_err)?;
            if let Some(hit) = hit_from_row(id, title, kind, path, 4) {
                hits.push(hit);
            }
        }
        Ok(hits)
    }

    pub fn snapshot(&self) -> Result<IndexSnapshot, VaultError> {
        let conn = lock(&self.conn)?;
        let mut notes = query_snapshot_notes(&conn)?;
        notes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut aliases = query_pairs(
            &conn,
            "SELECT note_id, alias FROM aliases ORDER BY note_id, alias",
        )?;
        aliases.sort();
        let mut links = super::graph::all_links(&conn)?;
        links.sort();
        Ok(IndexSnapshot {
            notes,
            aliases,
            links,
        })
    }
}

pub(crate) fn index_err(err: impl ToString) -> VaultError {
    VaultError::Index(err.to_string())
}

fn lock(conn: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>, VaultError> {
    conn.lock()
        .map_err(|err| VaultError::Index(err.to_string()))
}

fn ensure_schema(conn: &Connection) -> Result<(), VaultError> {
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(index_err)?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS notes_fts;
         DROP TABLE IF EXISTS links;
         DROP TABLE IF EXISTS aliases;
         DROP TABLE IF EXISTS notes;",
    )
    .map_err(index_err)?;
    conn.execute_batch(CREATE_SCHEMA).map_err(index_err)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(index_err)
}

fn insert_note(tx: &Transaction<'_>, note: &KnowledgeNote) -> Result<(), VaultError> {
    let id = index_note_id(note);
    let path = path_str(&note.path);
    let aliases = note.aliases.join("\n");
    let tags = note.tags.join("\n");
    tx.execute(
        "INSERT INTO notes (id, path, title, kind, content_hash, mtime, aliases, tags, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            path,
            note.title,
            note.kind.as_str(),
            note.content_hash,
            mtime_now(),
            aliases,
            tags,
            note.body
        ],
    )
    .map_err(index_err)?;
    insert_aliases(tx, &id, &note.aliases)?;
    insert_outgoing_links(tx, note, &id)?;
    retarget_wikilinks(tx, &id, &note.title, &note.aliases)?;
    Ok(())
}

fn upsert_note_tx(tx: &Transaction<'_>, note: &KnowledgeNote) -> Result<(), VaultError> {
    let id = index_note_id(note);
    let path = path_str(&note.path);
    let unchanged = tx
        .query_row(
            "SELECT 1 FROM notes WHERE id = ?1 AND path = ?2 AND content_hash = ?3",
            params![id, path, note.content_hash],
            |_| Ok(()),
        )
        .optional()
        .map_err(index_err)?
        .is_some();
    if unchanged {
        return Ok(());
    }
    let old_at_path: Option<String> = tx
        .query_row(
            "SELECT id FROM notes WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )
        .optional()
        .map_err(index_err)?;
    if let Some(old_id) = old_at_path
        && old_id != id
    {
        delete_note_id(tx, &old_id)?;
    }
    tx.execute("DELETE FROM aliases WHERE note_id = ?1", params![id])
        .map_err(index_err)?;
    tx.execute("DELETE FROM links WHERE source_id = ?1", params![id])
        .map_err(index_err)?;
    let aliases = note.aliases.join("\n");
    let tags = note.tags.join("\n");
    tx.execute(
        "INSERT INTO notes (id, path, title, kind, content_hash, mtime, aliases, tags, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            path = excluded.path,
            title = excluded.title,
            kind = excluded.kind,
            content_hash = excluded.content_hash,
            mtime = excluded.mtime,
            aliases = excluded.aliases,
            tags = excluded.tags,
            body = excluded.body",
        params![
            id,
            path,
            note.title,
            note.kind.as_str(),
            note.content_hash,
            mtime_now(),
            aliases,
            tags,
            note.body
        ],
    )
    .map_err(index_err)?;
    insert_aliases(tx, &id, &note.aliases)?;
    insert_outgoing_links(tx, note, &id)?;
    retarget_wikilinks(tx, &id, &note.title, &note.aliases)?;
    Ok(())
}

fn remove_path_tx(tx: &Transaction<'_>, relative: &Path) -> Result<(), VaultError> {
    let path = path_str(relative);
    let ids: Vec<String> = {
        let mut stmt = tx
            .prepare("SELECT id FROM notes WHERE path = ?1 OR path LIKE (?1 || '/%')")
            .map_err(index_err)?;
        let rows = stmt
            .query_map(params![path], |row| row.get(0))
            .map_err(index_err)?;
        let mut ids = Vec::new();
        for id in rows {
            ids.push(id.map_err(index_err)?);
        }
        ids
    };
    for id in ids {
        delete_note_id(tx, &id)?;
    }
    Ok(())
}

fn delete_note_id(tx: &Transaction<'_>, id: &str) -> Result<(), VaultError> {
    tx.execute("DELETE FROM aliases WHERE note_id = ?1", params![id])
        .map_err(index_err)?;
    tx.execute(
        "DELETE FROM links WHERE source_id = ?1 OR target_id = ?1",
        params![id],
    )
    .map_err(index_err)?;
    tx.execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(index_err)?;
    Ok(())
}

fn insert_aliases(tx: &Transaction<'_>, id: &str, aliases: &[String]) -> Result<(), VaultError> {
    for alias in aliases {
        let alias = alias.trim();
        if alias.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO aliases (note_id, alias) VALUES (?1, ?2)",
            params![id, alias],
        )
        .map_err(index_err)?;
    }
    Ok(())
}

fn insert_outgoing_links(
    tx: &Transaction<'_>,
    note: &KnowledgeNote,
    source_id: &str,
) -> Result<(), VaultError> {
    for kind in [
        RelationKind::Related,
        RelationKind::Requires,
        RelationKind::Uses,
        RelationKind::ProducedBy,
        RelationKind::DerivedFrom,
        RelationKind::Mentions,
        RelationKind::Supersedes,
    ] {
        for target in note.relations.ids_for(kind) {
            insert_link(tx, source_id, target.as_str(), kind.as_str())?;
        }
    }
    for link in parse_wikilinks(&note.body) {
        let target_id = resolve_wikilink(tx, &link.target)?;
        insert_link(tx, source_id, &target_id, "wikilink")?;
    }
    Ok(())
}

fn insert_link(
    tx: &Transaction<'_>,
    source_id: &str,
    target_id: &str,
    relation_type: &str,
) -> Result<(), VaultError> {
    tx.execute(
        "INSERT OR IGNORE INTO links (source_id, target_id, relation_type)
         VALUES (?1, ?2, ?3)",
        params![source_id, target_id, relation_type],
    )
    .map_err(index_err)?;
    Ok(())
}

fn resolve_wikilink(tx: &Transaction<'_>, target: &str) -> Result<String, VaultError> {
    let target = target.trim();
    if target.is_empty() {
        return Ok(unresolved_target(target));
    }
    let by_title: Option<String> = tx
        .query_row(
            "SELECT id FROM notes WHERE lower(title) = lower(?1) LIMIT 1",
            params![target],
            |row| row.get(0),
        )
        .optional()
        .map_err(index_err)?;
    if let Some(id) = by_title {
        return Ok(id);
    }
    let by_alias: Option<String> = tx
        .query_row(
            "SELECT note_id FROM aliases WHERE lower(alias) = lower(?1) LIMIT 1",
            params![target],
            |row| row.get(0),
        )
        .optional()
        .map_err(index_err)?;
    Ok(by_alias.unwrap_or_else(|| unresolved_target(target)))
}

fn retarget_wikilinks(
    tx: &Transaction<'_>,
    id: &str,
    title: &str,
    aliases: &[String],
) -> Result<(), VaultError> {
    let mut keys = vec![unresolved_target(title)];
    for alias in aliases {
        keys.push(unresolved_target(alias));
    }
    for key in keys {
        tx.execute(
            "UPDATE links SET target_id = ?1
             WHERE relation_type = 'wikilink' AND target_id = ?2",
            params![id, key],
        )
        .map_err(index_err)?;
    }
    Ok(())
}

fn unresolved_target(title: &str) -> String {
    format!("title:{}", title.trim().to_lowercase())
}

fn refresh_fts(tx: &Transaction<'_>) -> Result<(), VaultError> {
    tx.execute_batch(
        "DELETE FROM notes_fts;
         INSERT INTO notes_fts (id, title, aliases, tags, body)
         SELECT id, title, aliases, tags, body FROM notes;",
    )
    .map_err(index_err)
}

fn mtime_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn query_snapshot_notes(conn: &Connection) -> Result<Vec<SnapshotNote>, VaultError> {
    let mut stmt = conn
        .prepare("SELECT id, path, title, kind, content_hash FROM notes")
        .map_err(index_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SnapshotNote {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                kind: row.get(3)?,
                content_hash: row.get(4)?,
            })
        })
        .map_err(index_err)?;
    let mut notes = Vec::new();
    for row in rows {
        notes.push(row.map_err(index_err)?);
    }
    Ok(notes)
}

fn query_pairs(conn: &Connection, sql: &str) -> Result<Vec<(String, String)>, VaultError> {
    let mut stmt = conn.prepare(sql).map_err(index_err)?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(index_err)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(index_err)?);
    }
    Ok(out)
}

pub(crate) fn hit_from_row(
    id: String,
    title: String,
    kind: String,
    path: String,
    rank: i64,
) -> Option<SearchHit> {
    Some(SearchHit {
        id,
        title,
        kind: NoteKind::parse(&kind).ok()?,
        path: PathBuf::from(path),
        rank,
    })
}
