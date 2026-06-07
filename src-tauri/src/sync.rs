use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::device_id;

pub(crate) const SYNC_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncTodo {
    pub uuid: String,
    pub title: String,
    pub completed: bool,
    #[serde(default)]
    pub pinned: bool,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub updated_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncDocument {
    pub format_version: u32,
    pub device_id: String,
    pub generated_at: i64,
    pub todos: Vec<SyncTodo>,
}

pub(crate) fn build_document(
    connection: &Connection,
    generated_at: i64,
) -> Result<SyncDocument, String> {
    let device_id = device_id(connection).map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "
            SELECT uuid, title, completed, pinned, sort_order, created_at, updated_at,
                   completed_at, deleted_at, updated_by
            FROM todos
            ORDER BY uuid ASC
            ",
        )
        .map_err(database_error)?;
    let todos = statement
        .query_map([], map_sync_todo)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    Ok(SyncDocument {
        format_version: SYNC_FORMAT_VERSION,
        device_id,
        generated_at,
        todos,
    })
}

pub(crate) fn merge_documents(
    local: &SyncDocument,
    remote: &SyncDocument,
    generated_at: i64,
) -> Result<SyncDocument, String> {
    validate_document(local)?;
    validate_document(remote)?;

    let mut merged = BTreeMap::<String, SyncTodo>::new();
    for todo in local.todos.iter().chain(&remote.todos) {
        merged
            .entry(todo.uuid.clone())
            .and_modify(|current| {
                if compare_todos(todo, current).is_gt() {
                    *current = todo.clone();
                }
            })
            .or_insert_with(|| todo.clone());
    }

    Ok(SyncDocument {
        format_version: SYNC_FORMAT_VERSION,
        device_id: local.device_id.clone(),
        generated_at,
        todos: merged.into_values().collect(),
    })
}

pub(crate) fn merge_remote_document(
    connection: &mut Connection,
    remote: &SyncDocument,
    generated_at: i64,
) -> Result<SyncDocument, String> {
    let local = build_document(connection, generated_at)?;
    let merged = merge_documents(&local, remote, generated_at)?;
    let transaction = connection.transaction().map_err(database_error)?;

    for todo in &merged.todos {
        transaction
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, completed, pinned, sort_order, created_at, updated_at,
                    completed_at, deleted_at, updated_by
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(uuid) DO UPDATE SET
                    title = excluded.title,
                    completed = excluded.completed,
                    pinned = excluded.pinned,
                    sort_order = excluded.sort_order,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    completed_at = excluded.completed_at,
                    deleted_at = excluded.deleted_at,
                    updated_by = excluded.updated_by
                ",
                params![
                    todo.uuid,
                    todo.title.trim(),
                    todo.completed,
                    todo.pinned,
                    todo.sort_order,
                    todo.created_at,
                    todo.updated_at,
                    todo.completed_at,
                    todo.deleted_at,
                    todo.updated_by,
                ],
            )
            .map_err(database_error)?;
    }

    transaction.commit().map_err(database_error)?;
    Ok(merged)
}

pub(crate) fn validate_document(document: &SyncDocument) -> Result<(), String> {
    if document.format_version != SYNC_FORMAT_VERSION {
        return Err(format!("同步文件版本 {} 不受支持", document.format_version));
    }
    if Uuid::parse_str(&document.device_id).is_err() {
        return Err("同步文件 device_id 无效".to_string());
    }
    if document.generated_at < 0 {
        return Err("同步文件 generated_at 无效".to_string());
    }

    let mut uuids = HashSet::new();
    for todo in &document.todos {
        if Uuid::parse_str(&todo.uuid).is_err() {
            return Err(format!("同步任务 UUID 无效：{}", todo.uuid));
        }
        if Uuid::parse_str(&todo.updated_by).is_err() {
            return Err(format!("同步任务 updated_by 无效：{}", todo.updated_by));
        }
        if !uuids.insert(&todo.uuid) {
            return Err(format!("同步文件包含重复 UUID：{}", todo.uuid));
        }
        if todo.title.trim().is_empty() {
            return Err("同步文件包含空标题任务".to_string());
        }
        if todo.created_at < 0
            || todo.updated_at < 0
            || todo.completed_at.is_some_and(|value| value < 0)
            || todo.deleted_at.is_some_and(|value| value < 0)
        {
            return Err("同步文件包含无效时间戳".to_string());
        }
    }
    Ok(())
}

fn compare_todos(left: &SyncTodo, right: &SyncTodo) -> Ordering {
    left.updated_at
        .cmp(&right.updated_at)
        .then_with(|| left.deleted_at.is_some().cmp(&right.deleted_at.is_some()))
        .then_with(|| left.updated_by.cmp(&right.updated_by))
        .then_with(|| canonical_tie_break(left).cmp(&canonical_tie_break(right)))
}

fn canonical_tie_break(todo: &SyncTodo) -> (bool, bool, Option<i64>, i64, &str, i64) {
    (
        todo.completed,
        todo.pinned,
        todo.completed_at,
        todo.sort_order,
        todo.title.as_str(),
        todo.created_at,
    )
}

fn map_sync_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncTodo> {
    Ok(SyncTodo {
        uuid: row.get(0)?,
        title: row.get(1)?,
        completed: row.get::<_, i64>(2)? != 0,
        pinned: row.get::<_, i64>(3)? != 0,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        completed_at: row.get(7)?,
        deleted_at: row.get(8)?,
        updated_by: row.get(9)?,
    })
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};

    const DEVICE_A: &str = "00000000-0000-4000-8000-00000000000a";
    const DEVICE_B: &str = "00000000-0000-4000-8000-00000000000b";
    const TODO_ID: &str = "00000000-0000-4000-8000-000000000001";

    fn document(device_id: &str, todos: Vec<SyncTodo>) -> SyncDocument {
        SyncDocument {
            format_version: SYNC_FORMAT_VERSION,
            device_id: device_id.to_string(),
            generated_at: 20,
            todos,
        }
    }

    fn todo(title: &str, updated_at: i64, updated_by: &str) -> SyncTodo {
        SyncTodo {
            uuid: TODO_ID.to_string(),
            title: title.to_string(),
            completed: false,
            pinned: false,
            sort_order: 0,
            created_at: 1,
            updated_at,
            completed_at: None,
            deleted_at: None,
            updated_by: updated_by.to_string(),
        }
    }

    #[test]
    fn builds_a_versioned_document_with_persisted_device_id() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();

        let first = build_document(&connection, 10).unwrap();
        let second = build_document(&connection, 11).unwrap();

        assert_eq!(first.format_version, SYNC_FORMAT_VERSION);
        assert_eq!(first.device_id, second.device_id);
        assert!(Uuid::parse_str(&first.device_id).is_ok());
    }

    #[test]
    fn newer_update_wins_independent_of_merge_direction() {
        let older = document(DEVICE_A, vec![todo("older", 10, DEVICE_A)]);
        let newer = document(DEVICE_B, vec![todo("newer", 11, DEVICE_B)]);

        let left = merge_documents(&older, &newer, 30).unwrap();
        let right = merge_documents(&newer, &older, 30).unwrap();

        assert_eq!(left.todos[0].title, "newer");
        assert_eq!(left.todos, right.todos);
    }

    #[test]
    fn deletion_wins_an_equal_timestamp_conflict() {
        let active = document(DEVICE_B, vec![todo("active", 10, DEVICE_B)]);
        let mut deleted_todo = todo("deleted", 10, DEVICE_A);
        deleted_todo.deleted_at = Some(10);
        let deleted = document(DEVICE_A, vec![deleted_todo]);

        let merged = merge_documents(&active, &deleted, 30).unwrap();

        assert_eq!(merged.todos[0].deleted_at, Some(10));
    }

    #[test]
    fn device_id_stably_resolves_equal_time_sort_conflicts() {
        let mut first = todo("todo", 10, DEVICE_A);
        first.sort_order = 1024;
        let mut second = todo("todo", 10, DEVICE_B);
        second.sort_order = 2048;

        let left = merge_documents(
            &document(DEVICE_A, vec![first]),
            &document(DEVICE_B, vec![second]),
            30,
        )
        .unwrap();
        let right = merge_documents(
            &document(DEVICE_B, vec![todo_with_order(2048, DEVICE_B)]),
            &document(DEVICE_A, vec![todo_with_order(1024, DEVICE_A)]),
            30,
        )
        .unwrap();

        assert_eq!(left.todos[0].sort_order, 2048);
        assert_eq!(left.todos, right.todos);
    }

    #[test]
    fn combines_unique_records_from_both_devices() {
        let local = document(DEVICE_A, vec![todo("local", 10, DEVICE_A)]);
        let mut remote_todo = todo("remote", 10, DEVICE_B);
        remote_todo.uuid = "00000000-0000-4000-8000-000000000002".to_string();
        let remote = document(DEVICE_B, vec![remote_todo]);

        let merged = merge_documents(&local, &remote, 30).unwrap();

        assert_eq!(merged.todos.len(), 2);
        assert_eq!(merged.device_id, DEVICE_A);
    }

    #[test]
    fn persists_remote_updates_and_soft_deletes_by_uuid() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        let local_device = device_id(&connection).unwrap();
        let mut local_todo = todo("local", 10, &local_device);
        local_todo.uuid = TODO_ID.to_string();
        merge_remote_document(
            &mut connection,
            &document(&local_device, vec![local_todo]),
            20,
        )
        .unwrap();

        let mut remote_todo = todo("remote deleted", 11, DEVICE_B);
        remote_todo.pinned = true;
        remote_todo.deleted_at = Some(11);
        let merged =
            merge_remote_document(&mut connection, &document(DEVICE_B, vec![remote_todo]), 30)
                .unwrap();

        assert_eq!(merged.todos[0].title, "remote deleted");
        let persisted = build_document(&connection, 31).unwrap();
        assert_eq!(persisted.todos[0].deleted_at, Some(11));
        assert!(persisted.todos[0].pinned);
        assert_eq!(persisted.todos[0].updated_by, DEVICE_B);
    }

    #[test]
    fn reads_legacy_sync_document_without_pinned_field() {
        let json = format!(
            r#"{{
                "format_version": 1,
                "device_id": "{DEVICE_A}",
                "generated_at": 20,
                "todos": [{{
                    "uuid": "{TODO_ID}",
                    "title": "legacy",
                    "completed": false,
                    "sort_order": 0,
                    "created_at": 1,
                    "updated_at": 1,
                    "completed_at": null,
                    "deleted_at": null,
                    "updated_by": "{DEVICE_A}"
                }}]
            }}"#
        );

        let document: SyncDocument = serde_json::from_str(&json).unwrap();
        assert!(!document.todos[0].pinned);
        validate_document(&document).unwrap();
    }

    fn todo_with_order(sort_order: i64, updated_by: &str) -> SyncTodo {
        let mut value = todo("todo", 10, updated_by);
        value.sort_order = sort_order;
        value
    }
}
