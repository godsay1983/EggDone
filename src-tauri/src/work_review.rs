use crate::task_checklist_protocol::valid_uuid;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_TIME: i64 = 9_007_199_254_740_991;
const MAX_COPY_UNITS: usize = 100_000;
const SOURCE: &str = "FROM task_progress_entries p JOIN todos t ON t.uuid=p.task_uuid
    LEFT JOIN groups g ON g.uuid=t.group_uuid AND g.deleted_at IS NULL
    WHERE p.deleted_at IS NULL AND t.deleted_at IS NULL
    AND NOT EXISTS(SELECT 1 FROM lifecycle_terminals l WHERE l.kind='todo' AND l.uuid=t.uuid)
    AND p.created_at>=?1 AND p.created_at<?2
    AND (?3='all' OR (?3='ungrouped' AND g.uuid IS NULL) OR (?3='group' AND g.uuid=?4))
    AND (?5='' OR instr(lower(t.title),?5)>0 OR instr(lower(p.body),?5)>0)";
const COLUMNS: &str = "p.uuid,p.task_uuid,t.title,g.uuid,g.name,g.color,t.completed,
    t.archived_at IS NOT NULL,p.body,p.created_at,p.updated_at";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewQuery {
    pub start_at: i64,
    pub end_at: i64,
    pub group_scope: String,
    pub group_uuid: Option<String>,
    pub keyword: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewCursor {
    pub created_at: i64,
    pub record_uuid: String,
    pub query_key: String,
    pub snapshot_token: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReviewRow {
    pub record_uuid: String,
    pub task_uuid: String,
    pub task_title: String,
    pub group_uuid: Option<String>,
    pub group_name: Option<String>,
    pub group_color: Option<String>,
    pub completed: bool,
    pub archived: bool,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}
#[derive(Debug, Serialize)]
pub struct ReviewPage {
    pub rows: Vec<ReviewRow>,
    pub next_cursor: Option<ReviewCursor>,
    pub matching_entry_count: i64,
    pub matching_task_count: i64,
    pub snapshot_token: String,
}
#[derive(Debug, Serialize)]
pub struct ReviewSnapshot {
    pub rows: Vec<ReviewRow>,
    pub matching_entry_count: i64,
    pub matching_task_count: i64,
    pub snapshot_token: String,
}
fn database_error(_: rusqlite::Error) -> String {
    "REVIEW_DATABASE".into()
}
fn normalized(query: &ReviewQuery) -> Result<ReviewQuery, String> {
    let mut q = query.clone();
    q.keyword = q
        .keyword
        .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .to_ascii_lowercase();
    if q.start_at < 0
        || q.end_at <= q.start_at
        || q.end_at > MAX_TIME
        || q.keyword.contains('\0')
        || q.keyword.encode_utf16().count() > 100
        || !matches!(q.group_scope.as_str(), "all" | "ungrouped" | "group")
        || (q.group_scope == "group" && !q.group_uuid.as_deref().is_some_and(valid_uuid))
        || (q.group_scope != "group" && q.group_uuid.is_some())
    {
        return Err("REVIEW_INVALID".into());
    }
    Ok(q)
}
fn row(r: &Row<'_>) -> rusqlite::Result<ReviewRow> {
    Ok(ReviewRow {
        record_uuid: r.get(0)?,
        task_uuid: r.get(1)?,
        task_title: r.get(2)?,
        group_uuid: r.get(3)?,
        group_name: r.get(4)?,
        group_color: r.get(5)?,
        completed: r.get(6)?,
        archived: r.get(7)?,
        body: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}
fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "REVIEW_INVALID")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn counts(db: &Connection, q: &ReviewQuery) -> Result<(i64, i64), String> {
    db.query_row(
        &format!("SELECT COUNT(*),COUNT(DISTINCT p.task_uuid) {SOURCE}"),
        params![q.start_at, q.end_at, q.group_scope, q.group_uuid, q.keyword],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(database_error)
}
fn rows(
    db: &Connection,
    q: &ReviewQuery,
    cursor: Option<&ReviewCursor>,
    limit: Option<usize>,
) -> Result<Vec<ReviewRow>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} {SOURCE}
        AND (?6 IS NULL OR p.created_at<?6 OR (p.created_at=?6 AND p.uuid<?7))
        ORDER BY p.created_at DESC,p.uuid DESC LIMIT ?8"
        ))
        .map_err(database_error)?;
    let result = statement
        .query_map(
            params![
                q.start_at,
                q.end_at,
                q.group_scope,
                q.group_uuid,
                q.keyword,
                cursor.map(|c| c.created_at),
                cursor.map(|c| c.record_uuid.as_str()),
                limit.map(|v| v as i64).unwrap_or(-1)
            ],
            row,
        )
        .map_err(database_error)?;
    let mut output = Vec::new();
    let mut units = 0;
    for item in result {
        let item = item.map_err(database_error)?;
        if limit.is_none() {
            units += item.body.encode_utf16().count();
            if units > MAX_COPY_UNITS {
                return Err("REVIEW_LIMIT".into());
            }
        }
        output.push(item);
    }
    Ok(output)
}
fn snapshot_token(db: &Connection, q: &ReviewQuery) -> Result<String, String> {
    // Progress triggers track all record writes. Hash distinct parents as well: a title/group
    // change does not advance that revision. This avoids rehashing every long body on each page.
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(q).map_err(|_| "REVIEW_INVALID")?);
    let epoch: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key='sync.target.epoch.v1'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(database_error)?;
    hash.update(serde_json::to_vec(&epoch).map_err(|_| "REVIEW_DATABASE")?);
    let revision: (i64, i64) = db
        .query_row(
            "SELECT revision,generation FROM task_progress_sync_state WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(database_error)?;
    hash.update(revision.0.to_le_bytes());
    hash.update(revision.1.to_le_bytes());
    let mut statement = db
        .prepare(&format!(
            "SELECT DISTINCT t.uuid,t.title,g.uuid,g.name,g.color,t.completed,t.archived_at IS NOT NULL {SOURCE} ORDER BY t.uuid"
        ))
        .map_err(database_error)?;
    let result = statement
        .query_map(
            params![q.start_at, q.end_at, q.group_scope, q.group_uuid, q.keyword],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, bool>(5)?,
                    r.get::<_, bool>(6)?,
                ))
            },
        )
        .map_err(database_error)?;
    for item in result {
        hash.update(b"\n");
        hash.update(
            serde_json::to_vec(&item.map_err(database_error)?).map_err(|_| "REVIEW_DATABASE")?,
        );
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn list(
    db: &mut Connection,
    query: &ReviewQuery,
    cursor: Option<&ReviewCursor>,
) -> Result<ReviewPage, String> {
    let q = normalized(query)?;
    let key = digest(&q)?;
    if cursor.is_some_and(|c| {
        !valid_uuid(&c.record_uuid)
            || c.created_at < q.start_at
            || c.created_at >= q.end_at
            || !valid_token(&c.snapshot_token)
            || c.query_key != key
    }) {
        return Err("REVIEW_INVALID".into());
    }
    let tx = db.transaction().map_err(database_error)?;
    let token = snapshot_token(&tx, &q)?;
    if cursor.is_some_and(|c| c.snapshot_token != token) {
        return Err("REVIEW_CHANGED".into());
    }
    let (matching_entry_count, matching_task_count) = counts(&tx, &q)?;
    let mut result = rows(&tx, &q, cursor, Some(31))?;
    let more = result.len() > 30;
    result.truncate(30);
    let next_cursor = if more {
        result.last().map(|r| ReviewCursor {
            created_at: r.created_at,
            record_uuid: r.record_uuid.clone(),
            query_key: key,
            snapshot_token: token.clone(),
        })
    } else {
        None
    };
    tx.commit().map_err(database_error)?;
    Ok(ReviewPage {
        rows: result,
        next_cursor,
        matching_entry_count,
        matching_task_count,
        snapshot_token: token,
    })
}
pub fn snapshot(db: &mut Connection, query: &ReviewQuery) -> Result<ReviewSnapshot, String> {
    let q = normalized(query)?;
    let tx = db.transaction().map_err(database_error)?;
    let token = snapshot_token(&tx, &q)?;
    let (matching_entry_count, matching_task_count) = counts(&tx, &q)?;
    let result = rows(&tx, &q, None, None)?;
    tx.commit().map_err(database_error)?;
    Ok(ReviewSnapshot {
        rows: result,
        matching_entry_count,
        matching_task_count,
        snapshot_token: token,
    })
}
pub fn validate(db: &mut Connection, query: &ReviewQuery, expected: &str) -> Result<bool, String> {
    let q = normalized(query)?;
    if !valid_token(expected) {
        return Err("REVIEW_INVALID".into());
    }
    let tx = db.transaction().map_err(database_error)?;
    let current = snapshot_token(&tx, &q)?;
    tx.commit().map_err(database_error)?;
    Ok(current == expected)
}

#[cfg(test)]
#[path = "work_review_tests.rs"]
mod tests;
