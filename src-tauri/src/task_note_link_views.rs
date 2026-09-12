//! Scoped read models for the association UI. Reads never reconcile or delete links.
use crate::task_note_link_protocol::{link_uuid, parse_document, TaskNoteLink};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkScope {
    Todo,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityState {
    Missing,
    Deleted,
    Archived,
    Completed,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskNoteLinkView {
    pub link: TaskNoteLink,
    pub todo_title: Option<String>,
    pub note_title: Option<String>,
    pub todo_state: EntityState,
    pub note_state: EntityState,
    pub is_repeating: bool,
}

pub fn list(
    db: &Connection,
    scope: LinkScope,
    uuid: &str,
) -> Result<Vec<TaskNoteLinkView>, String> {
    link_uuid(uuid, uuid)?;
    let column = match scope {
        LinkScope::Todo => "todo_uuid",
        LinkScope::Note => "note_uuid",
    };
    let sql = format!("SELECT l.record_json,t.uuid AS todo_id,t.title AS todo_title,
        t.deleted_at AS todo_deleted,t.archived_at,t.completed,n.uuid AS note_id,n.title AS note_title,
        n.deleted_at AS note_deleted,(t.repeat_rule IS NOT NULL OR t.repeat_series_uuid IS NOT NULL) AS repeating,
        l.todo_uuid,l.note_uuid,l.uuid AS link_id
        FROM task_note_links l LEFT JOIN todos t ON t.uuid=l.todo_uuid
        LEFT JOIN notes n ON n.uuid=l.note_uuid WHERE l.active=1 AND l.{column}=?1");
    let mut query = db.prepare(&sql).map_err(db_error)?;
    let mut rows = query.query([uuid.to_ascii_lowercase()]).map_err(db_error)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(db_error)? {
        let record: String = row.get(0).map_err(db_error)?;
        let link = parse_record(&record)?;
        if link.deleted_at.is_some()
            || link.todo_uuid != row.get::<_, String>(10).map_err(db_error)?
            || link.note_uuid != row.get::<_, String>(11).map_err(db_error)?
            || link.uuid != row.get::<_, String>(12).map_err(db_error)?
        {
            return Err("TASK_NOTE_LINK_DATABASE_INCONSISTENT".into());
        }
        let exists = |index| {
            row.get::<_, Option<String>>(index)
                .map(|v| v.is_some())
                .map_err(db_error)
        };
        let present = |index| {
            row.get::<_, Option<i64>>(index)
                .map(|v| v.is_some())
                .map_err(db_error)
        };
        let todo_state = if !exists(1)? {
            EntityState::Missing
        } else if present(3)? {
            EntityState::Deleted
        } else if present(4)? {
            EntityState::Archived
        } else if row.get::<_, Option<i64>>(5).map_err(db_error)? == Some(1) {
            EntityState::Completed
        } else {
            EntityState::Active
        };
        let note_state = if !exists(6)? {
            EntityState::Missing
        } else if present(8)? {
            EntityState::Deleted
        } else {
            EntityState::Active
        };
        result.push(TaskNoteLinkView {
            todo_title: if todo_state == EntityState::Deleted {
                None
            } else {
                row.get(2).map_err(db_error)?
            },
            note_title: if note_state == EntityState::Deleted {
                None
            } else {
                row.get(7).map_err(db_error)?
            },
            is_repeating: row.get::<_, i64>(9).map_err(db_error)? == 1,
            link,
            todo_state,
            note_state,
        });
    }
    result
        .sort_by(|a, b| (a.link.created_at, &a.link.uuid).cmp(&(b.link.created_at, &b.link.uuid)));
    Ok(result)
}

pub fn pair(db: &Connection, todo: &str, note: &str) -> Result<Option<TaskNoteLink>, String> {
    use rusqlite::OptionalExtension;
    let key = link_uuid(todo, note)?;
    let record: Option<String> = db
        .query_row(
            "SELECT record_json FROM task_note_links WHERE uuid=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_error)?;
    record
        .map(|record| {
            let link = parse_record(&record)?;
            if link.uuid != key {
                return Err("TASK_NOTE_LINK_DATABASE_INCONSISTENT".into());
            }
            Ok(link)
        })
        .transpose()
}

fn db_error(error: rusqlite::Error) -> String {
    format!("TASK_NOTE_LINK_DATABASE: {error}")
}

fn parse_record(record: &str) -> Result<TaskNoteLink, String> {
    let mut doc = parse_document(&format!("{{\"format_version\":1,\"links\":[{record}]}}"))?;
    if doc.links.len() != 1 {
        return Err("TASK_NOTE_LINK_DATABASE_INCONSISTENT".into());
    }
    Ok(doc.links.remove(0))
}

#[cfg(test)]
#[path = "task_note_link_view_tests.rs"]
mod tests;
