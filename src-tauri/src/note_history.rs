use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct NoteTextVersion {
    pub title: String,
    pub content: String,
    pub updated_at: i64,
    pub updated_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub note_uuid: String,
    pub text: NoteTextVersion,
    pub captured_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistorySummary {
    pub id: i64,
    pub note_uuid: String,
    pub title: String,
    pub excerpt: String,
    pub updated_at: i64,
    pub captured_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HistoryPreview {
    pub entry: HistoryEntry,
    pub current: NoteTextVersion,
}

fn error(_: rusqlite::Error) -> String {
    "NOTE_HISTORY_DATABASE_FAILED".into()
}

fn identity(uuid: &str) -> Result<(), String> {
    if uuid.len() != 36 || uuid::Uuid::parse_str(uuid).is_err() {
        return Err("NOTE_HISTORY_INVALID_ID".into());
    }
    Ok(())
}

fn current(connection: &Connection, uuid: &str) -> Result<NoteTextVersion, String> {
    identity(uuid)?;
    connection.query_row(
        "SELECT title,content,updated_at,updated_by FROM notes WHERE uuid=? AND deleted_at IS NULL",
        [uuid], |row| Ok(NoteTextVersion { title: row.get(0)?, content: row.get(1)?,
            updated_at: row.get(2)?, updated_by: row.get(3)? }))
        .optional().map_err(error)?.ok_or_else(|| "NOTE_HISTORY_NOTE_UNAVAILABLE".into())
}

pub fn list(connection: &Connection, uuid: &str) -> Result<Vec<HistorySummary>, String> {
    let tx = connection.unchecked_transaction().map_err(error)?;
    current(&tx, uuid)?;
    let result = {
        let mut query = tx
            .prepare(
                "SELECT id,note_uuid,title,substr(content,1,120),updated_at,captured_at
            FROM note_history WHERE note_uuid=? ORDER BY id DESC LIMIT 100",
            )
            .map_err(error)?;
        let rows = query
            .query_map([uuid], |row| {
                Ok(HistorySummary {
                    id: row.get(0)?,
                    note_uuid: row.get(1)?,
                    title: row.get(2)?,
                    excerpt: row.get(3)?,
                    updated_at: row.get(4)?,
                    captured_at: row.get(5)?,
                })
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        rows
    };
    tx.commit().map_err(error)?;
    Ok(result)
}

fn preview_in(connection: &Connection, uuid: &str, id: i64) -> Result<HistoryPreview, String> {
    if !(1..=MAX_SAFE).contains(&id) {
        return Err("NOTE_HISTORY_INVALID_ID".into());
    }
    let current = current(connection, uuid)?;
    let entry = connection
        .query_row(
            "SELECT id,note_uuid,title,content,updated_at,updated_by,captured_at
        FROM note_history WHERE note_uuid=? AND id=?",
            params![uuid, id],
            |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    note_uuid: row.get(1)?,
                    text: NoteTextVersion {
                        title: row.get(2)?,
                        content: row.get(3)?,
                        updated_at: row.get(4)?,
                        updated_by: row.get(5)?,
                    },
                    captured_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(error)?
        .ok_or("NOTE_HISTORY_NOT_FOUND")?;
    Ok(HistoryPreview { entry, current })
}

pub fn preview(connection: &Connection, uuid: &str, id: i64) -> Result<HistoryPreview, String> {
    let tx = connection.unchecked_transaction().map_err(error)?;
    let result = preview_in(&tx, uuid, id)?;
    tx.commit().map_err(error)?;
    Ok(result)
}

/// The text capture trigger and the replacement commit or roll back together.
pub fn restore(
    connection: &mut Connection,
    expected: &HistoryPreview,
    now: i64,
    by: &str,
) -> Result<bool, String> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let actual = preview_in(&tx, &expected.entry.note_uuid, expected.entry.id)?;
    if &actual != expected {
        return Err("NOTE_HISTORY_CONFLICT".into());
    }
    let old = &actual.current;
    let target = &actual.entry.text;
    if old.title == target.title && old.content == target.content {
        tx.commit().map_err(error)?;
        return Ok(false);
    }
    if target.title.chars().count() > 100 || target.content.chars().count() > 20_000 {
        return Err("NOTE_HISTORY_INVALID_TEXT".into());
    }
    if [old.updated_at, target.updated_at, now]
        .iter()
        .any(|v| !(0..MAX_SAFE).contains(v))
        || by.trim().is_empty()
    {
        return Err("NOTE_HISTORY_INVALID_VERSION".into());
    }
    let stamp = now.max(old.updated_at.max(target.updated_at) + 1);
    tx.execute("UPDATE notes SET title=?,content=?,updated_at=?,updated_by=? WHERE uuid=? AND deleted_at IS NULL",
        params![target.title, target.content, stamp, by, actual.entry.note_uuid]).map_err(error)?;
    tx.commit().map_err(error)?;
    Ok(true)
}
