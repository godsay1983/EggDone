//! Local, bounded literal search. Only current entities are queried; no history or binary reads.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    Todo,
    Note,
    Attachment,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SearchItem {
    pub kind: SearchScope,
    pub uuid: String,
    pub title: String,
    pub excerpt: String,
    pub parent_uuid: Option<String>,
    pub parent_title: Option<String>,
    pub completed: bool,
    pub archived: bool,
    pub updated_at: i64,
    pub matched_field: String,
}

#[derive(Debug, Serialize)]
pub struct SearchPage {
    pub query: String,
    pub scope: SearchScope,
    pub offset: u32,
    pub limit: u32,
    pub total: i64,
    pub items: Vec<SearchItem>,
}

#[derive(Debug, Serialize)]
pub struct SearchTarget {
    pub kind: SearchScope,
    pub uuid: String,
    pub title: String,
    pub content: String,
    pub parent_uuid: Option<String>,
    pub parent_title: Option<String>,
    pub completed: bool,
    pub archived: bool,
}

fn source(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::Todo => "SELECT uuid,title,coalesce(note,'') AS body,NULL AS parent_uuid,NULL AS parent_title,
            completed,archived_at IS NOT NULL AS archived,updated_at FROM todos WHERE deleted_at IS NULL",
        SearchScope::Note => "SELECT uuid,title,content AS body,NULL AS parent_uuid,NULL AS parent_title,
            0 AS completed,0 AS archived,updated_at FROM notes WHERE deleted_at IS NULL",
        SearchScope::Attachment => "SELECT a.uuid,a.display_name AS title,n.content AS body,n.uuid AS parent_uuid,n.title AS parent_title,
            0 AS completed,0 AS archived,a.updated_at FROM note_attachments a JOIN notes n ON n.uuid=a.note_uuid
            WHERE a.deleted_at IS NULL AND n.deleted_at IS NULL",
    }
}

fn db_error(_: rusqlite::Error) -> String {
    "SEARCH_DATABASE_FAILED".into()
}

pub fn search(
    db: &Connection,
    scope: SearchScope,
    query: &str,
    offset: u32,
    limit: u32,
) -> Result<SearchPage, String> {
    let query = query.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if query.chars().count() > 100 || query.contains('\0') {
        return Err("SEARCH_INVALID_QUERY".into());
    }
    if offset > 100_000 || !(1..=50).contains(&limit) {
        return Err("SEARCH_INVALID_PAGE".into());
    }
    let mut result = SearchPage {
        query: query.into(),
        scope,
        offset,
        limit,
        total: 0,
        items: vec![],
    };
    if query.is_empty() {
        return Ok(result);
    }
    let body_hit = if scope == SearchScope::Attachment {
        "0"
    } else {
        "instr(lower(body),lower(?1))"
    };
    let matched = format!(
        "SELECT s.*,instr(lower(title),lower(?1)) AS title_hit,{body_hit} AS body_hit FROM ({}) s",
        source(scope)
    );
    let tx = db.unchecked_transaction().map_err(db_error)?;
    result.total = tx
        .query_row(
            &format!("SELECT count(*) FROM ({matched}) WHERE title_hit>0 OR body_hit>0"),
            [query],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    {
        let sql = format!("SELECT uuid,title,substr(body,max(1,body_hit-32),160),parent_uuid,parent_title,completed,archived,updated_at,title_hit
            FROM ({matched}) WHERE title_hit>0 OR body_hit>0 ORDER BY (title_hit>0) DESC,updated_at DESC,uuid ASC LIMIT ?2 OFFSET ?3");
        let mut statement = tx.prepare(&sql).map_err(db_error)?;
        result.items = statement
            .query_map(params![query, limit, offset], |row| {
                Ok(SearchItem {
                    kind: scope,
                    uuid: row.get(0)?,
                    title: row.get(1)?,
                    excerpt: row.get(2)?,
                    parent_uuid: row.get(3)?,
                    parent_title: row.get(4)?,
                    completed: row.get(5)?,
                    archived: row.get(6)?,
                    updated_at: row.get(7)?,
                    matched_field: if scope == SearchScope::Attachment {
                        "filename"
                    } else if row.get::<_, i64>(8)? > 0 {
                        "title"
                    } else {
                        "body"
                    }
                    .into(),
                })
            })
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
    }
    tx.commit().map_err(db_error)?;
    Ok(result)
}

/// Re-read the destination instead of trusting a stale search result. No file is opened here.
pub fn resolve(db: &Connection, scope: SearchScope, uuid: &str) -> Result<SearchTarget, String> {
    if uuid.len() != 36 || uuid::Uuid::parse_str(uuid).is_err() {
        return Err("SEARCH_INVALID_ID".into());
    }
    let sql = format!(
        "SELECT uuid,title,body,parent_uuid,parent_title,completed,archived FROM ({}) WHERE uuid=?",
        source(scope)
    );
    db.query_row(&sql, [uuid], |r| {
        Ok(SearchTarget {
            kind: scope,
            uuid: r.get(0)?,
            title: r.get(1)?,
            content: r.get(2)?,
            parent_uuid: r.get(3)?,
            parent_title: r.get(4)?,
            completed: r.get(5)?,
            archived: r.get(6)?,
        })
    })
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| "SEARCH_UNAVAILABLE".into())
}
