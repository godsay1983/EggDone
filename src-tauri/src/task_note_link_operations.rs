//! Local atomic commands. UI and transport are connected in later phases.
use crate::task_note_link_protocol::*;
use crate::task_note_link_store as store;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedTodoDraft {
    pub todo_uuid: String,
    pub note_uuid: String,
    pub title: String,
    pub note: String,
    pub group_uuid: Option<String>,
    pub due_date: Option<String>,
    pub due_at: Option<i64>,
    pub reminder_at: Option<i64>,
    pub priority: i64,
}

fn error(e: rusqlite::Error) -> String {
    format!("TASK_NOTE_LINK_DATABASE: {e}")
}
fn stamp(now: i64, observed: i64) -> Result<i64, String> {
    let next = now.max(observed.saturating_add(1));
    if !(0..=MAX_CLOCK).contains(&now) || !(0..=MAX_CLOCK).contains(&next) {
        return Err("TASK_NOTE_LINK_CLOCK_EXHAUSTED".into());
    }
    Ok(next)
}
fn identity(todo: &str, note: &str, by: &str) -> Result<TaskNoteLink, String> {
    let link = TaskNoteLink {
        uuid: link_uuid(todo, note)?,
        todo_uuid: todo.into(),
        note_uuid: note.into(),
        created_at: 0,
        updated_at: 0,
        updated_by: by.into(),
        deleted_at: None,
    };
    validate_link(&link)?;
    Ok(link)
}
fn active_entity(tx: &Connection, uuid: &str, note: bool) -> Result<i64, String> {
    let sql = if note {
        "SELECT updated_at FROM notes WHERE uuid=?1 AND deleted_at IS NULL"
    } else {
        "SELECT updated_at FROM todos WHERE uuid=?1 AND deleted_at IS NULL AND archived_at IS NULL"
    };
    tx.query_row(sql, [uuid], |r| r.get(0))
        .optional()
        .map_err(error)?
        .ok_or_else(|| "TASK_NOTE_LINK_ENTITY_UNAVAILABLE".into())
}

pub fn change(
    db: &mut Connection,
    todo: &str,
    note: &str,
    active: bool,
    expected: Option<&TaskNoteLink>,
    now: i64,
    by: &str,
) -> Result<Option<TaskNoteLink>, String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let result = change_in_transaction(&tx, todo, note, active, expected, now, by)?;
    tx.commit().map_err(error)?;
    Ok(result)
}

pub fn change_in_transaction(
    tx: &Transaction<'_>,
    todo: &str,
    note: &str,
    active: bool,
    expected: Option<&TaskNoteLink>,
    now: i64,
    by: &str,
) -> Result<Option<TaskNoteLink>, String> {
    let mut next = identity(todo, note, by)?;
    stamp(now, -1)?;
    if let Some(expected) = expected {
        validate_link(expected)?;
        if expected.uuid != next.uuid {
            return Err("TASK_NOTE_LINK_CONFLICT".into());
        }
    }
    let mut observed = 0;
    if active {
        observed = active_entity(tx, todo, false)?.max(active_entity(tx, note, true)?);
    }
    let snapshot = store::snapshot(tx)?;
    let current = snapshot
        .document
        .links
        .iter()
        .find(|item| item.uuid == next.uuid);
    // A duplicate desired state is a no-op; stale opposite-state requests never overwrite it.
    if current.is_some_and(|item| item.deleted_at.is_none() == active) {
        return Ok(current.cloned());
    }
    if current != expected {
        return Err("TASK_NOTE_LINK_CONFLICT".into());
    }
    if !active && current.is_none() {
        return Ok(None);
    }
    if active
        && snapshot
            .document
            .links
            .iter()
            .filter(|item| item.todo_uuid == todo && item.deleted_at.is_none())
            .count()
            >= MAX_LOCAL_LINKS_PER_TODO
    {
        return Err("TASK_NOTE_LINK_LIMIT".into());
    }
    if let Some(current) = current {
        next.created_at = current.created_at;
        observed = observed.max(current.updated_at);
    }
    next.updated_at = stamp(now, observed)?;
    if current.is_none() {
        next.created_at = next.updated_at;
    }
    next.deleted_at = if active { None } else { Some(next.updated_at) };
    store::merge_in_transaction(
        tx,
        &LinkDocument {
            format_version: 1,
            links: vec![next.clone()],
        },
    )?;
    Ok(Some(next))
}

pub fn create(
    db: &mut Connection,
    draft: &LinkedTodoDraft,
    now: i64,
    by: &str,
) -> Result<TaskNoteLink, String> {
    identity(&draft.todo_uuid, &draft.note_uuid, by)?;
    stamp(now, -1)?;
    if draft.title.trim().is_empty()
        || draft.title.encode_utf16().count() > 100
        || draft.note.encode_utf16().count() > 1000
        || !matches!(draft.priority, 0 | 1)
        || (draft.due_date.is_some() && draft.due_at.is_some())
        || [draft.due_at, draft.reminder_at]
            .into_iter()
            .flatten()
            .any(|v| !(0..=MAX_CLOCK).contains(&v))
    {
        return Err("TASK_NOTE_LINK_INVALID_DRAFT".into());
    }
    if crate::commands::normalize_due_date(draft.due_date.clone())? != draft.due_date {
        return Err("TASK_NOTE_LINK_INVALID_DRAFT".into());
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    active_entity(&tx, &draft.note_uuid, true)?;
    let key = format!("task_note_link_create.v1:{}", draft.todo_uuid);
    let payload = serde_json::to_string(draft).map_err(|e| e.to_string())?;
    let receipt: Option<String> = tx
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [&key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(error)?;
    if let Some(receipt) = receipt {
        if receipt != payload {
            return Err("TASK_NOTE_LINK_CONFLICT".into());
        }
        active_entity(&tx, &draft.todo_uuid, false)?;
        let result = store::snapshot(&tx)?
            .document
            .links
            .into_iter()
            .find(|l| l.todo_uuid == draft.todo_uuid && l.note_uuid == draft.note_uuid)
            .ok_or("TASK_NOTE_LINK_CONFLICT")?;
        tx.commit().map_err(error)?;
        return Ok(result);
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM todos WHERE uuid=?1)",
            [&draft.todo_uuid],
            |r| r.get(0),
        )
        .map_err(error)?;
    if exists {
        return Err("TASK_NOTE_LINK_CONFLICT".into());
    }
    if let Some(group) = &draft.group_uuid {
        let valid: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid=?1 AND deleted_at IS NULL)",
                [group],
                |r| r.get(0),
            )
            .map_err(error)?;
        if !valid {
            return Err("TASK_NOTE_LINK_INVALID_GROUP".into());
        }
    }
    let order: i64 = tx.query_row("SELECT COALESCE(MIN(sort_order),1024)-1024 FROM todos WHERE deleted_at IS NULL AND archived_at IS NULL", [], |r| r.get(0)).map_err(error)?;
    tx.execute("INSERT INTO todos(uuid,title,note,group_uuid,completed,pinned,priority,sort_order,created_at,updated_at,due_date,due_at,reminder_at,updated_by)
      VALUES(?1,?2,?3,?4,0,0,?5,?6,?7,?7,?8,?9,?10,?11)",
      params![draft.todo_uuid,draft.title.trim(),draft.note,draft.group_uuid,draft.priority,order,now,draft.due_date,draft.due_at,draft.reminder_at,by]).map_err(error)?;
    let result =
        change_in_transaction(&tx, &draft.todo_uuid, &draft.note_uuid, true, None, now, by)?
            .ok_or("TASK_NOTE_LINK_CONFLICT")?;
    tx.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![key, payload],
    )
    .map_err(error)?;
    tx.commit().map_err(error)?;
    Ok(result)
}
