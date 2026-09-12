use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    Todo,
    Note,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrashAttachment {
    pub uuid: String,
    pub name: String,
    pub updated_at: i64,
    pub updated_by: String,
    pub deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrashItem {
    pub kind: TrashKind,
    pub uuid: String,
    pub title: String,
    pub content: String,
    pub deleted_at: i64,
    pub updated_at: i64,
    pub updated_by: String,
    pub completed: bool,
    pub repeating: bool,
    pub attachments: Vec<TrashAttachment>,
}

fn error(_: rusqlite::Error) -> String {
    "TRASH_DATABASE_FAILED".into()
}

pub fn list(connection: &Connection, offset: u32, limit: u32) -> Result<Vec<TrashItem>, String> {
    if limit == 0 || limit > 100 {
        return Err("TRASH_INVALID_PAGE".into());
    }
    let tx = connection.unchecked_transaction().map_err(error)?;
    let keys = {
        let mut query = tx
            .prepare(
                "SELECT kind,uuid FROM (
          SELECT 'todo' AS kind,uuid,deleted_at FROM todos WHERE deleted_at IS NOT NULL
          UNION ALL SELECT 'note',uuid,deleted_at FROM notes WHERE deleted_at IS NOT NULL)
          ORDER BY deleted_at DESC,kind ASC,uuid ASC LIMIT ?1 OFFSET ?2",
            )
            .map_err(error)?;
        let result = query
            .query_map(params![limit, offset], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        result
    };
    let mut result = Vec::new();
    for (kind, uuid) in keys {
        result.push(preview_in(
            &tx,
            if kind == "todo" {
                TrashKind::Todo
            } else {
                TrashKind::Note
            },
            &uuid,
        )?);
    }
    tx.commit().map_err(error)?;
    Ok(result)
}

pub fn preview(connection: &Connection, kind: TrashKind, uuid: &str) -> Result<TrashItem, String> {
    let tx = connection.unchecked_transaction().map_err(error)?;
    let result = preview_in(&tx, kind, uuid)?;
    tx.commit().map_err(error)?;
    Ok(result)
}

fn preview_in(connection: &Connection, kind: TrashKind, uuid: &str) -> Result<TrashItem, String> {
    uuid::Uuid::parse_str(uuid).map_err(|_| "TRASH_INVALID_ID".to_string())?;
    let sql = match kind {
        TrashKind::Todo => "SELECT uuid,title,COALESCE(note,''),deleted_at,updated_at,updated_by,completed,
          (repeat_rule IS NOT NULL OR repeat_series_uuid IS NOT NULL OR EXISTS
           (SELECT 1 FROM recurrence_rules r WHERE r.current_todo_uuid=todos.uuid))
          FROM todos WHERE uuid=?1 AND deleted_at IS NOT NULL",
        TrashKind::Note => "SELECT uuid,title,content,deleted_at,updated_at,updated_by,0 AS completed,0 AS repeating
          FROM notes WHERE uuid=?1 AND deleted_at IS NOT NULL",
    };
    let mut item = connection
        .query_row(sql, [uuid], |row| {
            Ok(TrashItem {
                kind,
                uuid: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                deleted_at: row.get(3)?,
                updated_at: row.get(4)?,
                updated_by: row.get(5)?,
                completed: row.get(6)?,
                repeating: row.get(7)?,
                attachments: Vec::new(),
            })
        })
        .optional()
        .map_err(error)?
        .ok_or("TRASH_NOT_FOUND")?;
    if kind == TrashKind::Note {
        let mut query = connection
            .prepare(
                "SELECT uuid,display_name,updated_at,updated_by,deleted_at
          FROM note_attachments WHERE note_uuid=?1 AND (deleted_at IS NULL OR deleted_at=?2)
          ORDER BY uuid ASC",
            )
            .map_err(error)?;
        item.attachments = query
            .query_map(params![uuid, item.deleted_at], |row| {
                Ok(TrashAttachment {
                    uuid: row.get(0)?,
                    name: row.get(1)?,
                    updated_at: row.get(2)?,
                    updated_by: row.get(3)?,
                    deleted_at: row.get(4)?,
                })
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
    }
    Ok(item)
}

pub fn restore(
    connection: &mut Connection,
    expected: &TrashItem,
    now: i64,
    by: &str,
) -> Result<(), String> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    let current = preview_in(&tx, expected.kind, &expected.uuid)?;
    if &current != expected {
        return Err("TRASH_CONFLICT".into());
    }
    let observed = current
        .attachments
        .iter()
        .fold(current.updated_at.max(current.deleted_at), |stamp, item| {
            stamp.max(item.updated_at)
        });
    let stamp = now.max(observed.saturating_add(1));
    if now < 0 || !(0..=9_007_199_254_740_991).contains(&stamp) || by.trim().is_empty() {
        return Err("TRASH_INVALID_VERSION".into());
    }
    // Restore an item, never restart a rule or revive an old task-note relationship.
    if current.kind == TrashKind::Todo {
        let running: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_rules WHERE current_todo_uuid=?1 AND active=1)",
            [&current.uuid], |row| row.get(0)).map_err(error)?;
        if running {
            return Err("TRASH_RULE_ACTIVE".into());
        }
    }
    crate::task_note_link_store::tombstone_entity(
        &tx,
        &current.uuid,
        current.kind == TrashKind::Note,
        stamp,
        by,
    )?;
    match current.kind {
        TrashKind::Note => {
            // Existing migration triggers restore only attachments deleted with this note.
            tx.execute(
                "UPDATE notes SET deleted_at=NULL,updated_at=?1,updated_by=?2 WHERE uuid=?3",
                params![stamp, by, current.uuid],
            )
            .map_err(error)?;
        }
        TrashKind::Todo => {
            tx.execute("UPDATE todos SET deleted_at=NULL,archived_at=NULL,updated_at=?1,updated_by=?2,
              reminder_at=NULL,repeat_rule=NULL,repeat_next_due_date=NULL,repeat_series_uuid=NULL,
              group_uuid=CASE WHEN EXISTS(SELECT 1 FROM groups WHERE groups.uuid=todos.group_uuid AND deleted_at IS NULL)
                THEN group_uuid ELSE NULL END WHERE uuid=?3", params![stamp, by, current.uuid]).map_err(error)?;
        }
    }
    tx.commit().map_err(error)?;
    Ok(())
}
