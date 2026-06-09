use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use uuid::Uuid;

use crate::{
    db::{device_id, now_millis, Database},
    s3_sync::{
        self, ConnectionTestResult, ManualSyncResult, SaveSyncSettings, SyncRuntime, SyncSettings,
        UploadOutcome,
    },
    sync::{self, SyncDocument},
    tray::{self, PanelState},
};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Todo {
    id: i64,
    uuid: String,
    title: String,
    group_uuid: Option<String>,
    completed: bool,
    pinned: bool,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
    deleted_at: Option<i64>,
    due_date: Option<String>,
    due_at: Option<i64>,
    reminder_at: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TodoGroup {
    id: i64,
    uuid: String,
    name: String,
    color: String,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

#[tauri::command]
pub fn list_todos(database: State<'_, Database>) -> Result<Vec<Todo>, String> {
    let connection = lock_database(&database)?;
    list_todos_from_connection(&connection)
}

#[tauri::command]
pub fn list_groups(database: State<'_, Database>) -> Result<Vec<TodoGroup>, String> {
    let connection = lock_database(&database)?;
    list_groups_from_connection(&connection)
}

#[tauri::command]
pub fn create_todo(
    title: String,
    group_uuid: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        create_todo_in_connection(&connection, &title, group_uuid)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn create_group(
    name: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoGroup, String> {
    let result = {
        let connection = lock_database(&database)?;
        create_group_in_connection(&connection, &name)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn update_group_name(
    uuid: String,
    name: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoGroup, String> {
    let result = {
        let connection = lock_database(&database)?;
        update_group_name_in_connection(&connection, &uuid, &name)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn update_group_color(
    uuid: String,
    color: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoGroup, String> {
    let result = {
        let connection = lock_database(&database)?;
        update_group_color_in_connection(&connection, &uuid, &color)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn delete_group(
    uuid: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoGroup, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        delete_group_in_connection(&mut connection, &uuid)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn reorder_groups(
    ordered_uuids: Vec<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Vec<TodoGroup>, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        reorder_groups_in_connection(&mut connection, &ordered_uuids)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_todo_completed(
    id: i64,
    completed: bool,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        set_todo_completed_in_connection(&connection, id, completed)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn update_todo_title(
    id: i64,
    title: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        update_todo_title_in_connection(&connection, id, &title)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_todo_pinned(
    id: i64,
    pinned: bool,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        set_todo_pinned_in_connection(&connection, id, pinned)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_todo_schedule(
    id: i64,
    due_date: Option<String>,
    due_at: Option<i64>,
    reminder_at: Option<i64>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        set_todo_schedule_in_connection(&connection, id, due_date, due_at, reminder_at)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_todo_group(
    id: i64,
    group_uuid: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        set_todo_group_in_connection(&connection, id, group_uuid)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn reorder_todos(
    ordered_ids: Vec<i64>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Vec<Todo>, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        reorder_todos_in_connection(&mut connection, &ordered_ids)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn delete_todo(id: i64, app: AppHandle, database: State<'_, Database>) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        soft_delete_todo_in_connection(&connection, id)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn restore_todo(
    id: i64,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        restore_todo_in_connection(&connection, id)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn clear_completed_todos(
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<usize, String> {
    let result = {
        let connection = lock_database(&database)?;
        clear_completed_todos_in_connection(&connection)
    };
    refresh_badge_after_success(&app, &result);
    result
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

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    database: State<'_, Database>,
    runtime: State<'_, SyncRuntime>,
) -> Result<ManualSyncResult, String> {
    let _guard = runtime.acquire()?;
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_manual_sync(&connection)?
    };
    let mut remote = s3_sync::download_remote(&prepared).await?;

    for attempt in 0..=1 {
        let merged = {
            let mut connection = lock_database(&database)?;
            match &remote.document {
                Some(document) => {
                    sync::merge_remote_document(&mut connection, document, now_millis())?
                }
                None => sync::build_document(&connection, now_millis())?,
            }
        };
        tray::update_task_badge(&app);
        let _ = app.emit_to("main", "todos-changed", ());

        match s3_sync::upload_document(&prepared, &merged, &remote).await? {
            UploadOutcome::Success => {
                return Ok(ManualSyncResult {
                    message: if attempt == 0 {
                        "同步完成".to_string()
                    } else {
                        "检测到远端更新，重新合并后同步完成".to_string()
                    },
                    todo_count: merged.todos.len(),
                    conflict_retried: attempt > 0,
                });
            }
            UploadOutcome::Conflict if attempt == 0 => {
                remote = s3_sync::download_remote(&prepared).await?;
            }
            UploadOutcome::Conflict => {
                return Err("远端文件持续发生变化，已停止上传并保留本地数据".to_string());
            }
        }
    }

    Err("同步未完成".to_string())
}

fn refresh_badge_after_success<T>(app: &AppHandle, result: &Result<T, String>) {
    if result.is_ok() {
        tray::update_task_badge(app);
    }
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
                id, uuid, title, group_uuid, completed, pinned, sort_order,
                created_at, updated_at, completed_at, deleted_at,
                due_date, due_at, reminder_at
            FROM todos
            WHERE deleted_at IS NULL
            ORDER BY completed ASC, pinned DESC, sort_order ASC, created_at DESC, id DESC
            ",
        )
        .map_err(database_error)?;

    let rows = statement.query_map([], map_todo).map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn list_groups_from_connection(connection: &Connection) -> Result<Vec<TodoGroup>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, uuid, name, color, sort_order, created_at, updated_at, deleted_at
            FROM groups
            WHERE deleted_at IS NULL
            ORDER BY sort_order ASC, created_at ASC, id ASC
            ",
        )
        .map_err(database_error)?;

    let rows = statement.query_map([], map_group).map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn create_todo_in_connection(
    connection: &Connection,
    title: &str,
    group_uuid: Option<String>,
) -> Result<Todo, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务内容不能为空".to_string());
    }
    let group_uuid = normalize_group_uuid(connection, group_uuid)?;

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
                uuid, title, group_uuid, completed, sort_order,
                created_at, updated_at, completed_at, deleted_at,
                due_date, due_at, reminder_at, updated_by
            )
            VALUES (?1, ?2, ?3, 0, ?4, ?5, ?5, NULL, NULL, NULL, NULL, NULL, ?6)
            ",
            params![uuid, title, group_uuid, sort_order, now, updated_by],
        )
        .map_err(database_error)?;

    find_todo(connection, connection.last_insert_rowid())?
        .ok_or_else(|| "新建任务后未能读取记录".to_string())
}

fn create_group_in_connection(connection: &Connection, name: &str) -> Result<TodoGroup, String> {
    let name = normalize_group_name(name)?;
    let now = now_millis();
    let uuid = Uuid::new_v4().to_string();
    let sort_order: i64 = connection
        .query_row(
            "
            SELECT COALESCE(MAX(sort_order), -1024) + 1024
            FROM groups
            WHERE deleted_at IS NULL
            ",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;

    connection
        .execute(
            "
            INSERT INTO groups (
                uuid, name, color, sort_order, created_at, updated_at, deleted_at, updated_by
            )
            VALUES (?1, ?2, 'yellow', ?3, ?4, ?4, NULL, ?5)
            ",
            params![
                uuid,
                name,
                sort_order,
                now,
                device_id(connection).map_err(database_error)?,
            ],
        )
        .map_err(database_error)?;

    find_group(connection, connection.last_insert_rowid())?
        .ok_or_else(|| "新建分组后未能读取记录".to_string())
}

fn update_group_name_in_connection(
    connection: &Connection,
    uuid: &str,
    name: &str,
) -> Result<TodoGroup, String> {
    let uuid = normalize_existing_group_uuid(connection, uuid)?;
    let name = normalize_group_name(name)?;
    let changed = connection
        .execute(
            "
            UPDATE groups
            SET name = ?1, updated_at = ?2, updated_by = ?3
            WHERE uuid = ?4 AND deleted_at IS NULL
            ",
            params![
                name,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                uuid,
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("分组不存在".to_string());
    }

    find_group_by_uuid(connection, &uuid)?.ok_or_else(|| "重命名后未能读取分组".to_string())
}

fn update_group_color_in_connection(
    connection: &Connection,
    uuid: &str,
    color: &str,
) -> Result<TodoGroup, String> {
    let uuid = normalize_existing_group_uuid(connection, uuid)?;
    let color = normalize_group_color(color)?;
    let changed = connection
        .execute(
            "
            UPDATE groups
            SET color = ?1, updated_at = ?2, updated_by = ?3
            WHERE uuid = ?4 AND deleted_at IS NULL
            ",
            params![
                color,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                uuid,
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("分组不存在".to_string());
    }

    find_group_by_uuid(connection, &uuid)?.ok_or_else(|| "改色后未能读取分组".to_string())
}

fn delete_group_in_connection(
    connection: &mut Connection,
    uuid: &str,
) -> Result<TodoGroup, String> {
    let uuid = normalize_existing_group_uuid(connection, uuid)?;
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let deleted_group = transaction
        .query_row(
            "
            UPDATE groups
            SET deleted_at = ?1, updated_at = ?1, updated_by = ?2
            WHERE uuid = ?3 AND deleted_at IS NULL
            RETURNING id, uuid, name, color, sort_order, created_at, updated_at, deleted_at
            ",
            params![now, updated_by, uuid],
            map_group,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "分组不存在".to_string())?;

    transaction
        .execute(
            "
            UPDATE todos
            SET group_uuid = NULL, updated_at = ?1, updated_by = ?2
            WHERE group_uuid = ?3 AND deleted_at IS NULL
            ",
            params![now, updated_by, uuid],
        )
        .map_err(database_error)?;
    transaction.commit().map_err(database_error)?;

    Ok(deleted_group)
}

fn reorder_groups_in_connection(
    connection: &mut Connection,
    ordered_uuids: &[String],
) -> Result<Vec<TodoGroup>, String> {
    if ordered_uuids.is_empty() {
        return Ok(list_groups_from_connection(connection)?);
    }

    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    for (index, uuid) in ordered_uuids.iter().enumerate() {
        let uuid = normalize_group_uuid_format(uuid)?;
        let changed = transaction
            .execute(
                "
                UPDATE groups
                SET sort_order = ?1, updated_at = ?2, updated_by = ?3
                WHERE uuid = ?4 AND deleted_at IS NULL
                ",
                params![index as i64 * 1024, now, updated_by, uuid],
            )
            .map_err(database_error)?;
        if changed == 0 {
            return Err("排序中包含不存在的分组".to_string());
        }
    }
    transaction.commit().map_err(database_error)?;

    list_groups_from_connection(connection)
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

fn set_todo_pinned_in_connection(
    connection: &Connection,
    id: i64,
    pinned: bool,
) -> Result<Todo, String> {
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET pinned = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
            ",
            params![
                pinned,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                id
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "置顶后未能读取任务".to_string())
}

fn set_todo_schedule_in_connection(
    connection: &Connection,
    id: i64,
    due_date: Option<String>,
    due_at: Option<i64>,
    reminder_at: Option<i64>,
) -> Result<Todo, String> {
    let due_date = normalize_due_date(due_date)?;
    if due_at.is_some_and(|value| value < 0) || reminder_at.is_some_and(|value| value < 0) {
        return Err("到期或提醒时间无效".to_string());
    }
    if due_date.is_some() && due_at.is_some() {
        return Err("纯日期任务不能同时设置具体到期时间".to_string());
    }

    let changed = connection
        .execute(
            "
            UPDATE todos
            SET due_date = ?1, due_at = ?2, reminder_at = ?3,
                updated_at = ?4, updated_by = ?5
            WHERE id = ?6 AND deleted_at IS NULL
            ",
            params![
                due_date,
                due_at,
                reminder_at,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                id
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "设置日期后未能读取任务".to_string())
}

fn set_todo_group_in_connection(
    connection: &Connection,
    id: i64,
    group_uuid: Option<String>,
) -> Result<Todo, String> {
    let group_uuid = normalize_group_uuid(connection, group_uuid)?;
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET group_uuid = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
            ",
            params![
                group_uuid,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                id
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "移动分组后未能读取任务".to_string())
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
                id, uuid, title, group_uuid, completed, pinned, sort_order,
                created_at, updated_at, completed_at, deleted_at,
                due_date, due_at, reminder_at
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
        group_uuid: row.get(3)?,
        completed: row.get::<_, i64>(4)? != 0,
        pinned: row.get::<_, i64>(5)? != 0,
        sort_order: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        completed_at: row.get(9)?,
        deleted_at: row.get(10)?,
        due_date: row.get(11)?,
        due_at: row.get(12)?,
        reminder_at: row.get(13)?,
    })
}

fn find_group(connection: &Connection, id: i64) -> Result<Option<TodoGroup>, String> {
    connection
        .query_row(
            "
            SELECT id, uuid, name, color, sort_order, created_at, updated_at, deleted_at
            FROM groups
            WHERE id = ?1
            ",
            params![id],
            map_group,
        )
        .optional()
        .map_err(database_error)
}

fn find_group_by_uuid(connection: &Connection, uuid: &str) -> Result<Option<TodoGroup>, String> {
    connection
        .query_row(
            "
            SELECT id, uuid, name, color, sort_order, created_at, updated_at, deleted_at
            FROM groups
            WHERE uuid = ?1
            ",
            params![uuid],
            map_group,
        )
        .optional()
        .map_err(database_error)
}

fn map_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoGroup> {
    Ok(TodoGroup {
        id: row.get(0)?,
        uuid: row.get(1)?,
        name: row.get(2)?,
        color: row.get(3)?,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        deleted_at: row.get(7)?,
    })
}

fn normalize_group_uuid(
    connection: &Connection,
    group_uuid: Option<String>,
) -> Result<Option<String>, String> {
    let Some(value) = group_uuid else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let trimmed = normalize_group_uuid_format(trimmed)?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid = ?1 AND deleted_at IS NULL)",
            params![trimmed],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if !exists {
        return Err("分组不存在".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

fn normalize_existing_group_uuid(connection: &Connection, uuid: &str) -> Result<String, String> {
    let uuid = normalize_group_uuid_format(uuid.trim())?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid = ?1 AND deleted_at IS NULL)",
            params![uuid],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if !exists {
        return Err("分组不存在".to_string());
    }
    Ok(uuid)
}

fn normalize_group_uuid_format(uuid: &str) -> Result<String, String> {
    if Uuid::parse_str(uuid).is_err() {
        return Err("分组 UUID 无效".to_string());
    }
    Ok(uuid.to_string())
}

fn normalize_group_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分组名称不能为空".to_string());
    }
    if name.chars().count() > 30 {
        return Err("分组名称不能超过 30 个字符".to_string());
    }
    Ok(name.to_string())
}

fn normalize_group_color(color: &str) -> Result<&str, String> {
    let color = color.trim();
    match color {
        "yellow" | "green" | "blue" | "peach" | "lavender" | "gray" => Ok(color),
        _ => Err("分组颜色不在预设范围内".to_string()),
    }
}

fn normalize_due_date(due_date: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = due_date else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !is_valid_date_only(trimmed) {
        return Err("日期格式应为 YYYY-MM-DD".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

fn is_valid_date_only(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }

    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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

        let created = create_todo_in_connection(&connection, "  新任务  ", None).unwrap();
        assert_eq!(created.title, "新任务");
        assert_eq!(created.group_uuid, None);
        assert!(!created.completed);
        assert!(!created.pinned);
        assert!(Uuid::parse_str(&created.uuid).is_ok());
        assert_eq!(created.completed_at, None);
        assert_eq!(created.deleted_at, None);
        assert_eq!(created.due_date, None);
        assert_eq!(created.due_at, None);
        assert_eq!(created.reminder_at, None);
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
        let first = create_todo_in_connection(&connection, "first", None).unwrap();
        let second = create_todo_in_connection(&connection, "second", None).unwrap();

        let todos = list_todos_from_connection(&connection).unwrap();
        assert_eq!(todos[0].id, second.id);
        assert_eq!(todos[1].id, first.id);
        assert!(second.sort_order < first.sort_order);
    }

    #[test]
    fn edits_reorders_and_clears_completed_todos() {
        let mut connection = connection();
        let first = create_todo_in_connection(&connection, "first", None).unwrap();
        let second = create_todo_in_connection(&connection, "second", None).unwrap();

        let edited = update_todo_title_in_connection(&connection, first.id, "  edited  ").unwrap();
        assert_eq!(edited.title, "edited");
        assert!(update_todo_title_in_connection(&connection, first.id, " ").is_err());

        let pinned = set_todo_pinned_in_connection(&connection, first.id, true).unwrap();
        assert!(pinned.pinned);
        assert_eq!(
            list_todos_from_connection(&connection).unwrap()[0].id,
            first.id
        );

        let scheduled = set_todo_schedule_in_connection(
            &connection,
            first.id,
            Some("2026-06-10".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(scheduled.due_date.as_deref(), Some("2026-06-10"));
        assert!(set_todo_schedule_in_connection(
            &connection,
            first.id,
            Some("2026/06/10".to_string()),
            None,
            None
        )
        .is_err());
        assert!(set_todo_schedule_in_connection(
            &connection,
            first.id,
            Some("2026-02-31".to_string()),
            None,
            None
        )
        .is_err());

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

    #[test]
    fn creates_groups_and_moves_todos_between_them() {
        let connection = connection();
        let group = create_group_in_connection(&connection, "  工作  ").unwrap();
        assert_eq!(group.name, "工作");
        assert!(Uuid::parse_str(&group.uuid).is_ok());

        let created =
            create_todo_in_connection(&connection, "grouped", Some(group.uuid.clone())).unwrap();
        assert_eq!(created.group_uuid.as_deref(), Some(group.uuid.as_str()));

        let ungrouped = set_todo_group_in_connection(&connection, created.id, None).unwrap();
        assert_eq!(ungrouped.group_uuid, None);

        assert!(create_todo_in_connection(
            &connection,
            "bad group",
            Some("00000000-0000-4000-8000-00000000ffff".to_string()),
        )
        .is_err());
    }

    #[test]
    fn renames_reorders_and_deletes_groups_without_deleting_todos() {
        let mut connection = connection();
        let first = create_group_in_connection(&connection, "工作").unwrap();
        let second = create_group_in_connection(&connection, "生活").unwrap();
        let grouped =
            create_todo_in_connection(&connection, "grouped", Some(first.uuid.clone())).unwrap();

        let renamed =
            update_group_name_in_connection(&connection, &first.uuid, "  深度工作  ").unwrap();
        assert_eq!(renamed.name, "深度工作");

        let recolored =
            update_group_color_in_connection(&connection, &first.uuid, "green").unwrap();
        assert_eq!(recolored.color, "green");
        assert!(update_group_color_in_connection(&connection, &first.uuid, "neon").is_err());

        let reordered = reorder_groups_in_connection(
            &mut connection,
            &[second.uuid.clone(), first.uuid.clone()],
        )
        .unwrap();
        assert_eq!(reordered[0].uuid, second.uuid);
        assert_eq!(reordered[1].uuid, first.uuid);

        let deleted = delete_group_in_connection(&mut connection, &first.uuid).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert_eq!(list_groups_from_connection(&connection).unwrap().len(), 1);
        assert_eq!(
            find_todo(&connection, grouped.id)
                .unwrap()
                .unwrap()
                .group_uuid,
            None
        );
    }
}
