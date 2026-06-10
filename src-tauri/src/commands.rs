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
    repeat_rule: Option<String>,
    repeat_next_due_date: Option<String>,
    repeat_series_uuid: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TodoCompletion {
    updated_todo: Todo,
    created_todo: Option<Todo>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TodoDeletion {
    deleted_todos: Vec<Todo>,
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
) -> Result<TodoCompletion, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        set_todo_completed_in_connection(&mut connection, id, completed)
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
    repeat_rule: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        set_todo_schedule_in_connection(&connection, id, due_date, due_at, reminder_at, repeat_rule)
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
pub fn delete_todo(
    id: i64,
    repeat_scope: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoDeletion, String> {
    let result = {
        let connection = lock_database(&database)?;
        soft_delete_todo_in_connection(&connection, id, repeat_scope.as_deref())
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
                due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid
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
                due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid, updated_by
            )
            VALUES (?1, ?2, ?3, 0, ?4, ?5, ?5, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?6)
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
    connection: &mut Connection,
    id: i64,
    completed: bool,
) -> Result<TodoCompletion, String> {
    let before = find_todo(connection, id)?.ok_or_else(|| "任务不存在".to_string())?;
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let changed = transaction
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

    let created_id = if completed && !before.completed {
        create_next_repeat_instance(&transaction, &before, now, &updated_by)?
    } else {
        None
    };
    transaction.commit().map_err(database_error)?;

    let updated_todo =
        find_todo(connection, id)?.ok_or_else(|| "更新后未能读取任务".to_string())?;
    let created_todo = created_id
        .map(|id| find_todo(connection, id))
        .transpose()?
        .flatten();
    Ok(TodoCompletion {
        updated_todo,
        created_todo,
    })
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
    repeat_rule: Option<String>,
) -> Result<Todo, String> {
    let due_date = normalize_due_date(due_date)?;
    let repeat_rule = normalize_repeat_rule(repeat_rule)?;
    if due_at.is_some_and(|value| value < 0) || reminder_at.is_some_and(|value| value < 0) {
        return Err("到期或提醒时间无效".to_string());
    }
    if due_date.is_some() && due_at.is_some() {
        return Err("纯日期任务不能同时设置具体到期时间".to_string());
    }
    if repeat_rule.is_some() && due_date.is_none() {
        return Err("重复任务需要先设置到期日期".to_string());
    }
    if repeat_rule.is_some() && due_at.is_some() {
        return Err("重复任务暂只支持日期级到期".to_string());
    }
    let current = find_todo(connection, id)?.ok_or_else(|| "任务不存在".to_string())?;
    let repeat_next_due_date = match (&due_date, &repeat_rule) {
        (Some(date), Some(rule)) => Some(next_repeat_due_date(date, rule)?),
        _ => None,
    };
    let repeat_series_uuid = repeat_rule
        .as_ref()
        .map(|_| current.repeat_series_uuid.unwrap_or(current.uuid));

    let changed = connection
        .execute(
            "
            UPDATE todos
            SET due_date = ?1, due_at = ?2, reminder_at = ?3,
                repeat_rule = ?4, repeat_next_due_date = ?5,
                repeat_series_uuid = ?6,
                updated_at = ?7, updated_by = ?8
            WHERE id = ?9 AND deleted_at IS NULL
            ",
            params![
                due_date,
                due_at,
                reminder_at,
                repeat_rule,
                repeat_next_due_date,
                repeat_series_uuid,
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

fn soft_delete_todo_in_connection(
    connection: &Connection,
    id: i64,
    repeat_scope: Option<&str>,
) -> Result<TodoDeletion, String> {
    let target = find_todo(connection, id)?
        .filter(|todo| todo.deleted_at.is_none())
        .ok_or_else(|| "任务不存在".to_string())?;
    let scope = match repeat_scope.unwrap_or("single") {
        "single" => "single",
        "series" => "series",
        _ => return Err("删除范围无效".to_string()),
    };
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;

    let ids = if scope == "series" {
        let series_uuid = target
            .repeat_series_uuid
            .as_deref()
            .unwrap_or(target.uuid.as_str());
        let mut statement = connection
            .prepare(
                "
                SELECT id
                FROM todos
                WHERE deleted_at IS NULL
                    AND (repeat_series_uuid = ?1 OR uuid = ?1)
                ORDER BY created_at ASC, id ASC
                ",
            )
            .map_err(database_error)?;
        let ids = statement
            .query_map(params![series_uuid], |row| row.get::<_, i64>(0))
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;
        ids
    } else {
        vec![id]
    };

    if ids.is_empty() {
        return Err("任务不存在".to_string());
    }

    let mut changed = 0;
    for todo_id in &ids {
        changed += connection
            .execute(
                "
                UPDATE todos
                SET deleted_at = ?1, updated_at = ?1, updated_by = ?2
                WHERE id = ?3 AND deleted_at IS NULL
                ",
                params![now, updated_by, todo_id],
            )
            .map_err(database_error)?;
    }

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    let deleted_todos = ids
        .into_iter()
        .map(|todo_id| {
            find_todo(connection, todo_id)?.ok_or_else(|| "删除后未能读取任务".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TodoDeletion { deleted_todos })
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
                due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid
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
        repeat_rule: row.get(14)?,
        repeat_next_due_date: row.get(15)?,
        repeat_series_uuid: row.get(16)?,
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

fn normalize_repeat_rule(repeat_rule: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = repeat_rule else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed {
        "daily" | "weekly" | "monthly" | "weekdays" => Ok(Some(trimmed.to_string())),
        _ => Err("重复规则无效".to_string()),
    }
}

fn create_next_repeat_instance(
    connection: &Connection,
    source: &Todo,
    now: i64,
    updated_by: &str,
) -> Result<Option<i64>, String> {
    let Some(rule) = source.repeat_rule.as_deref() else {
        return Ok(None);
    };
    let Some(next_due_date) = source.repeat_next_due_date.as_deref() else {
        return Ok(None);
    };
    let next_repeat_next_due_date = next_repeat_due_date(next_due_date, rule)?;
    let repeat_series_uuid = source
        .repeat_series_uuid
        .as_deref()
        .unwrap_or(source.uuid.as_str());
    let uuid = Uuid::new_v4().to_string();
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
                uuid, title, group_uuid, completed, pinned, sort_order,
                created_at, updated_at, completed_at, deleted_at,
                due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid, updated_by
            )
            VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?6, NULL, NULL, ?7, NULL, NULL, ?8, ?9, ?10, ?11)
            ",
            params![
                uuid,
                source.title,
                source.group_uuid,
                source.pinned,
                sort_order,
                now,
                next_due_date,
                rule,
                next_repeat_next_due_date,
                repeat_series_uuid,
                updated_by,
            ],
        )
        .map_err(database_error)?;

    Ok(Some(connection.last_insert_rowid()))
}

fn next_repeat_due_date(date: &str, rule: &str) -> Result<String, String> {
    let (year, month, day) = parse_date_only(date)?;
    let next = match rule {
        "daily" => add_days(year, month, day, 1),
        "weekly" => add_days(year, month, day, 7),
        "monthly" => add_month(year, month, day),
        "weekdays" => {
            let mut candidate = add_days(year, month, day, 1);
            while weekday(candidate.0, candidate.1, candidate.2) >= 6 {
                candidate = add_days(candidate.0, candidate.1, candidate.2, 1);
            }
            candidate
        }
        _ => return Err("重复规则无效".to_string()),
    };
    Ok(format!("{:04}-{:02}-{:02}", next.0, next.1, next.2))
}

fn parse_date_only(value: &str) -> Result<(u32, u32, u32), String> {
    if !is_valid_date_only(value) {
        return Err("日期格式应为 YYYY-MM-DD".to_string());
    }
    let year = value[0..4]
        .parse::<u32>()
        .map_err(|_| "日期格式应为 YYYY-MM-DD".to_string())?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|_| "日期格式应为 YYYY-MM-DD".to_string())?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|_| "日期格式应为 YYYY-MM-DD".to_string())?;
    Ok((year, month, day))
}

fn add_days(mut year: u32, mut month: u32, mut day: u32, days: u32) -> (u32, u32, u32) {
    for _ in 0..days {
        day += 1;
        let max_day = days_in_month(year, month);
        if day > max_day {
            day = 1;
            month += 1;
            if month > 12 {
                month = 1;
                year += 1;
            }
        }
    }
    (year, month, day)
}

fn add_month(mut year: u32, mut month: u32, day: u32) -> (u32, u32, u32) {
    month += 1;
    if month > 12 {
        month = 1;
        year += 1;
    }
    (year, month, day.min(days_in_month(year, month)))
}

fn weekday(year: u32, month: u32, day: u32) -> u32 {
    let (mut year, mut month) = (year as i32, month as i32);
    if month < 3 {
        month += 12;
        year -= 1;
    }
    let century = year / 100;
    let year_of_century = year % 100;
    let h = (day as i32
        + ((13 * (month + 1)) / 5)
        + year_of_century
        + (year_of_century / 4)
        + (century / 4)
        + (5 * century))
        % 7;
    match h {
        0 => 6,
        1 => 7,
        value => (value - 1) as u32,
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
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
    (1..=days_in_month(year, month)).contains(&day)
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
        let mut connection = connection();

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
        assert_eq!(created.repeat_rule, None);
        assert_eq!(created.repeat_next_due_date, None);
        assert_eq!(created.repeat_series_uuid, None);
        assert_eq!(list_todos_from_connection(&connection).unwrap().len(), 1);

        let completed = set_todo_completed_in_connection(&mut connection, created.id, true)
            .unwrap()
            .updated_todo;
        assert!(completed.completed);
        assert!(completed.completed_at.is_some());
        assert!(completed.updated_at >= created.updated_at);

        let reopened = set_todo_completed_in_connection(&mut connection, created.id, false)
            .unwrap()
            .updated_todo;
        assert!(!reopened.completed);
        assert_eq!(reopened.completed_at, None);

        let deleted = soft_delete_todo_in_connection(&connection, created.id, None)
            .unwrap()
            .deleted_todos
            .remove(0);
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
            None,
        )
        .unwrap();
        assert_eq!(scheduled.due_date.as_deref(), Some("2026-06-10"));
        assert!(set_todo_schedule_in_connection(
            &connection,
            first.id,
            Some("2026/06/10".to_string()),
            None,
            None,
            None
        )
        .is_err());
        assert!(set_todo_schedule_in_connection(
            &connection,
            first.id,
            Some("2026-02-31".to_string()),
            None,
            None,
            None
        )
        .is_err());
        let repeating = set_todo_schedule_in_connection(
            &connection,
            first.id,
            Some("2026-06-10".to_string()),
            None,
            None,
            Some("daily".to_string()),
        )
        .unwrap();
        assert_eq!(repeating.repeat_rule.as_deref(), Some("daily"));
        assert_eq!(
            repeating.repeat_next_due_date.as_deref(),
            Some("2026-06-11")
        );
        assert_eq!(
            repeating.repeat_series_uuid.as_deref(),
            Some(repeating.uuid.as_str())
        );

        let reordered =
            reorder_todos_in_connection(&mut connection, &[first.id, second.id]).unwrap();
        assert_eq!(reordered[0].id, first.id);
        assert_eq!(reordered[1].id, second.id);

        set_todo_completed_in_connection(&mut connection, first.id, true).unwrap();
        assert_eq!(clear_completed_todos_in_connection(&connection).unwrap(), 1);
        let remaining = list_todos_from_connection(&connection).unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].due_date.as_deref(), Some("2026-06-11"));
        assert!(remaining.iter().any(|todo| todo.id == second.id));
    }

    #[test]
    fn completing_repeating_todo_creates_only_the_next_instance() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "repeat", None).unwrap();
        let repeating = set_todo_schedule_in_connection(
            &connection,
            created.id,
            Some("2026-06-12".to_string()),
            None,
            None,
            Some("weekdays".to_string()),
        )
        .unwrap();
        assert_eq!(
            repeating.repeat_next_due_date.as_deref(),
            Some("2026-06-15")
        );

        let result = set_todo_completed_in_connection(&mut connection, created.id, true).unwrap();
        assert!(result.updated_todo.completed);
        let next = result.created_todo.expect("next repeat instance");
        assert_eq!(next.title, "repeat");
        assert!(!next.completed);
        assert_eq!(next.due_date.as_deref(), Some("2026-06-15"));
        assert_eq!(next.repeat_rule.as_deref(), Some("weekdays"));
        assert_eq!(next.repeat_next_due_date.as_deref(), Some("2026-06-16"));
        assert_eq!(
            next.repeat_series_uuid.as_deref(),
            Some(repeating.uuid.as_str())
        );
        assert_eq!(list_todos_from_connection(&connection).unwrap().len(), 2);
    }

    #[test]
    fn deleting_repeating_series_soft_deletes_all_instances() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "repeat", None).unwrap();
        let repeating = set_todo_schedule_in_connection(
            &connection,
            created.id,
            Some("2026-06-10".to_string()),
            None,
            None,
            Some("daily".to_string()),
        )
        .unwrap();
        let next = set_todo_completed_in_connection(&mut connection, repeating.id, true)
            .unwrap()
            .created_todo
            .expect("next repeat instance");

        let deleted = soft_delete_todo_in_connection(&connection, next.id, Some("series"))
            .unwrap()
            .deleted_todos;

        assert_eq!(deleted.len(), 2);
        assert!(deleted.iter().all(|todo| todo.deleted_at.is_some()));
        assert!(list_todos_from_connection(&connection).unwrap().is_empty());
    }

    #[test]
    fn repeat_date_calculation_clamps_months_and_skips_weekends() {
        assert_eq!(
            next_repeat_due_date("2026-01-31", "monthly").unwrap(),
            "2026-02-28"
        );
        assert_eq!(
            next_repeat_due_date("2028-01-31", "monthly").unwrap(),
            "2028-02-29"
        );
        assert_eq!(
            next_repeat_due_date("2026-06-12", "weekdays").unwrap(),
            "2026-06-15"
        );
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
