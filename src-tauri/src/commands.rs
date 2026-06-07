use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use uuid::Uuid;

use crate::{
    db::{device_id, now_millis, Database},
    s3_sync::{self, ConnectionTestResult, SaveSyncSettings, SyncSettings},
    sync::{self, SyncDocument},
    tray::PanelState,
};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Todo {
    id: i64,
    uuid: String,
    title: String,
    completed: bool,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
    deleted_at: Option<i64>,
}

#[tauri::command]
pub fn list_todos(database: State<'_, Database>) -> Result<Vec<Todo>, String> {
    let connection = lock_database(&database)?;
    list_todos_from_connection(&connection)
}

#[tauri::command]
pub fn create_todo(title: String, database: State<'_, Database>) -> Result<Todo, String> {
    let connection = lock_database(&database)?;
    create_todo_in_connection(&connection, &title)
}

#[tauri::command]
pub fn set_todo_completed(
    id: i64,
    completed: bool,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let connection = lock_database(&database)?;
    set_todo_completed_in_connection(&connection, id, completed)
}

#[tauri::command]
pub fn update_todo_title(
    id: i64,
    title: String,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let connection = lock_database(&database)?;
    update_todo_title_in_connection(&connection, id, &title)
}

#[tauri::command]
pub fn reorder_todos(
    ordered_ids: Vec<i64>,
    database: State<'_, Database>,
) -> Result<Vec<Todo>, String> {
    let mut connection = lock_database(&database)?;
    reorder_todos_in_connection(&mut connection, &ordered_ids)
}

#[tauri::command]
pub fn delete_todo(id: i64, database: State<'_, Database>) -> Result<Todo, String> {
    let connection = lock_database(&database)?;
    soft_delete_todo_in_connection(&connection, id)
}

#[tauri::command]
pub fn restore_todo(id: i64, database: State<'_, Database>) -> Result<Todo, String> {
    let connection = lock_database(&database)?;
    restore_todo_in_connection(&connection, id)
}

#[tauri::command]
pub fn clear_completed_todos(database: State<'_, Database>) -> Result<usize, String> {
    let connection = lock_database(&database)?;
    clear_completed_todos_in_connection(&connection)
}

#[tauri::command]
pub fn hide_panel(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn mark_panel_interaction(panel_state: State<'_, PanelState>) {
    panel_state.mark_internal_interaction();
}

#[tauri::command]
pub fn toggle_panel_from_shortcut(app: AppHandle) {
    if crate::tray::toggle_panel(&app, None) {
        let _ = app.emit_to("main", "focus-new-todo", ());
    }
}

#[tauri::command]
pub fn prepare_sync_document(database: State<'_, Database>) -> Result<SyncDocument, String> {
    let connection = lock_database(&database)?;
    sync::build_document(&connection, now_millis())
}

#[tauri::command]
pub fn apply_remote_sync_document(
    remote: SyncDocument,
    database: State<'_, Database>,
) -> Result<SyncDocument, String> {
    let mut connection = lock_database(&database)?;
    sync::merge_remote_document(&mut connection, &remote, now_millis())
}

#[tauri::command]
pub fn get_sync_settings(database: State<'_, Database>) -> Result<SyncSettings, String> {
    let connection = lock_database(&database)?;
    s3_sync::get_settings(&connection)
}

#[tauri::command]
pub fn save_sync_settings(
    settings: SaveSyncSettings,
    database: State<'_, Database>,
) -> Result<SyncSettings, String> {
    let connection = lock_database(&database)?;
    s3_sync::save_settings(&connection, settings)
}

#[tauri::command]
pub fn delete_sync_credentials(database: State<'_, Database>) -> Result<(), String> {
    let connection = lock_database(&database)?;
    s3_sync::delete_credentials(&connection)
}

#[tauri::command]
pub async fn test_sync_connection(
    database: State<'_, Database>,
) -> Result<ConnectionTestResult, String> {
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_connection_test(&connection)?
    };
    s3_sync::test_connection(prepared).await
}

fn lock_database<'a>(
    database: &'a State<'_, Database>,
) -> Result<std::sync::MutexGuard<'a, Connection>, String> {
    database
        .connection
        .lock()
        .map_err(|_| "数据库锁不可用".to_string())
}

fn list_todos_from_connection(connection: &Connection) -> Result<Vec<Todo>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT
                id, uuid, title, completed, sort_order,
                created_at, updated_at, completed_at, deleted_at
            FROM todos
            WHERE deleted_at IS NULL
            ORDER BY completed ASC, sort_order ASC, created_at DESC, id DESC
            ",
        )
        .map_err(database_error)?;

    let rows = statement.query_map([], map_todo).map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn create_todo_in_connection(connection: &Connection, title: &str) -> Result<Todo, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务内容不能为空".to_string());
    }

    let now = now_millis();
    let uuid = Uuid::new_v4().to_string();
    let updated_by = device_id(connection).map_err(database_error)?;
    let sort_order: i64 = connection
        .query_row(
            "
            SELECT COALESCE(MIN(sort_order), 1024) - 1024
            FROM todos
            WHERE deleted_at IS NULL
            ",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;

    connection
        .execute(
            "
            INSERT INTO todos (
                uuid, title, completed, sort_order,
                created_at, updated_at, completed_at, deleted_at, updated_by
            )
            VALUES (?1, ?2, 0, ?3, ?4, ?4, NULL, NULL, ?5)
            ",
            params![uuid, title, sort_order, now, updated_by],
        )
        .map_err(database_error)?;

    find_todo(connection, connection.last_insert_rowid())?
        .ok_or_else(|| "新建任务后未能读取记录".to_string())
}

fn set_todo_completed_in_connection(
    connection: &Connection,
    id: i64,
    completed: bool,
) -> Result<Todo, String> {
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET
                completed = ?1,
                completed_at = CASE WHEN ?1 = 1 THEN ?2 ELSE NULL END,
                updated_at = ?2,
                updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
            ",
            params![completed, now, updated_by, id],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "更新后未能读取任务".to_string())
}

fn update_todo_title_in_connection(
    connection: &Connection,
    id: i64,
    title: &str,
) -> Result<Todo, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务内容不能为空".to_string());
    }

    let changed = connection
        .execute(
            "
            UPDATE todos
            SET title = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
            ",
            params![
                title,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                id
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "编辑后未能读取任务".to_string())
}

fn reorder_todos_in_connection(
    connection: &mut Connection,
    ordered_ids: &[i64],
) -> Result<Vec<Todo>, String> {
    if ordered_ids.is_empty() {
        return Ok(list_todos_from_connection(connection)?);
    }

    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    for (index, id) in ordered_ids.iter().enumerate() {
        let changed = transaction
            .execute(
                "
                UPDATE todos
                SET sort_order = ?1, updated_at = ?2, updated_by = ?3
                WHERE id = ?4 AND deleted_at IS NULL
                ",
                params![index as i64 * 1024, now, updated_by, id],
            )
            .map_err(database_error)?;
        if changed == 0 {
            return Err("排序中包含不存在的任务".to_string());
        }
    }
    transaction.commit().map_err(database_error)?;

    list_todos_from_connection(connection)
}

fn soft_delete_todo_in_connection(connection: &Connection, id: i64) -> Result<Todo, String> {
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET deleted_at = ?1, updated_at = ?1, updated_by = ?2
            WHERE id = ?3 AND deleted_at IS NULL
            ",
            params![now, updated_by, id],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "删除后未能读取任务".to_string())
}

fn restore_todo_in_connection(connection: &Connection, id: i64) -> Result<Todo, String> {
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET deleted_at = NULL, updated_at = ?1, updated_by = ?2
            WHERE id = ?3 AND deleted_at IS NOT NULL
            ",
            params![
                now_millis(),
                device_id(connection).map_err(database_error)?,
                id
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务未删除或不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "恢复后未能读取任务".to_string())
}

fn clear_completed_todos_in_connection(connection: &Connection) -> Result<usize, String> {
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    connection
        .execute(
            "
            UPDATE todos
            SET deleted_at = ?1, updated_at = ?1, updated_by = ?2
            WHERE completed = 1 AND deleted_at IS NULL
            ",
            params![now, updated_by],
        )
        .map_err(database_error)
}

fn find_todo(connection: &Connection, id: i64) -> Result<Option<Todo>, String> {
    connection
        .query_row(
            "
            SELECT
                id, uuid, title, completed, sort_order,
                created_at, updated_at, completed_at, deleted_at
            FROM todos
            WHERE id = ?1
            ",
            params![id],
            map_todo,
        )
        .optional()
        .map_err(database_error)
}

fn map_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: row.get(0)?,
        uuid: row.get(1)?,
        title: row.get(2)?,
        completed: row.get::<_, i64>(3)? != 0,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        completed_at: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    #[test]
    fn todo_lifecycle_uses_sync_ready_fields() {
        let connection = connection();

        let created = create_todo_in_connection(&connection, "  新任务  ").unwrap();
        assert_eq!(created.title, "新任务");
        assert!(!created.completed);
        assert!(Uuid::parse_str(&created.uuid).is_ok());
        assert_eq!(created.completed_at, None);
        assert_eq!(created.deleted_at, None);
        assert_eq!(list_todos_from_connection(&connection).unwrap().len(), 1);

        let completed = set_todo_completed_in_connection(&connection, created.id, true).unwrap();
        assert!(completed.completed);
        assert!(completed.completed_at.is_some());
        assert!(completed.updated_at >= created.updated_at);

        let reopened = set_todo_completed_in_connection(&connection, created.id, false).unwrap();
        assert!(!reopened.completed);
        assert_eq!(reopened.completed_at, None);

        let deleted = soft_delete_todo_in_connection(&connection, created.id).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(list_todos_from_connection(&connection).unwrap().is_empty());

        let restored = restore_todo_in_connection(&connection, created.id).unwrap();
        assert_eq!(restored.deleted_at, None);
        assert_eq!(list_todos_from_connection(&connection).unwrap().len(), 1);
    }

    #[test]
    fn newer_todos_are_inserted_before_existing_todos() {
        let connection = connection();
        let first = create_todo_in_connection(&connection, "first").unwrap();
        let second = create_todo_in_connection(&connection, "second").unwrap();

        let todos = list_todos_from_connection(&connection).unwrap();
        assert_eq!(todos[0].id, second.id);
        assert_eq!(todos[1].id, first.id);
        assert!(second.sort_order < first.sort_order);
    }

    #[test]
    fn edits_reorders_and_clears_completed_todos() {
        let mut connection = connection();
        let first = create_todo_in_connection(&connection, "first").unwrap();
        let second = create_todo_in_connection(&connection, "second").unwrap();

        let edited = update_todo_title_in_connection(&connection, first.id, "  edited  ").unwrap();
        assert_eq!(edited.title, "edited");
        assert!(update_todo_title_in_connection(&connection, first.id, " ").is_err());

        let reordered =
            reorder_todos_in_connection(&mut connection, &[first.id, second.id]).unwrap();
        assert_eq!(reordered[0].id, first.id);
        assert_eq!(reordered[1].id, second.id);

        set_todo_completed_in_connection(&connection, first.id, true).unwrap();
        assert_eq!(clear_completed_todos_in_connection(&connection).unwrap(), 1);
        let remaining = list_todos_from_connection(&connection).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, second.id);
    }
}
