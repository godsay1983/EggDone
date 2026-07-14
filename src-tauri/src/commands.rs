use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Size, State, WebviewWindow};
use uuid::Uuid;

use crate::{
    db::{device_id, now_millis, Database},
    note_asset_store::NoteAssetStore,
    note_attachment_sync, note_attachments, note_sync,
    notes::{self, Note},
    reminders,
    s3_sync::{
        self, AssetUploadOutcome, ConnectionTestResult, ManualSyncResult, SaveSyncSettings,
        SyncRuntime, SyncSettings, UploadOutcome,
    },
    schedule::{local_date_from_timestamp, timestamp_for_local_date},
    sync::{self, SyncDocument},
    tray::{self, PanelState},
};

const TODO_NOTE_MAX_CHARS: usize = 1000;
const FOCUS_WINDOW_WIDTH: f64 = 320.0;
const FOCUS_WINDOW_HEIGHT: f64 = 430.0;
const FOCUS_WINDOW_MIN_WIDTH: f64 = 300.0;
const FOCUS_WINDOW_MIN_HEIGHT: f64 = 390.0;
const FOCUS_COMPACT_WIDTH: f64 = 288.0;
const FOCUS_COMPACT_HEIGHT: f64 = 86.0;
const FOCUS_COMPACT_MIN_WIDTH: f64 = 260.0;
const FOCUS_COMPACT_MIN_HEIGHT: f64 = 80.0;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Todo {
    id: i64,
    uuid: String,
    title: String,
    note: Option<String>,
    group_uuid: Option<String>,
    completed: bool,
    pinned: bool,
    priority: i64,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
    deleted_at: Option<i64>,
    archived_at: Option<i64>,
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
pub struct TodoEdit {
    updated_todos: Vec<Todo>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatEditScope {
    Single,
    Future,
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
pub fn list_notes(database: State<'_, Database>) -> Result<Vec<Note>, String> {
    let connection = lock_database(&database)?;
    notes::list_active(&connection)
}

#[tauri::command]
pub fn create_note(
    title: String,
    content: String,
    color: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Note, String> {
    let result = {
        let connection = lock_database(&database)?;
        notes::create(&connection, &title, &content, &color)
    };
    emit_notes_changed_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn update_note(
    uuid: String,
    title: String,
    content: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Note, String> {
    let result = {
        let connection = lock_database(&database)?;
        notes::update(&connection, &uuid, &title, &content)
    };
    emit_notes_changed_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_note_pinned(
    uuid: String,
    pinned: bool,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Note, String> {
    let result = {
        let connection = lock_database(&database)?;
        notes::set_pinned(&connection, &uuid, pinned)
    };
    emit_notes_changed_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_note_color(
    uuid: String,
    color: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Note, String> {
    let result = {
        let connection = lock_database(&database)?;
        notes::set_color(&connection, &uuid, &color)
    };
    emit_notes_changed_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn delete_note(
    uuid: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Note, String> {
    let result = {
        let connection = lock_database(&database)?;
        notes::soft_delete(&connection, &uuid)
    };
    emit_notes_changed_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn restore_note(
    uuid: String,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Note, String> {
    let result = {
        let connection = lock_database(&database)?;
        notes::restore(&connection, &uuid)
    };
    emit_notes_changed_after_success(&app, &result);
    result
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
pub fn set_todo_completed_by_uuid(
    uuid: String,
    completed: bool,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoCompletion, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        let id =
            find_todo_id_by_uuid(&connection, &uuid)?.ok_or_else(|| "任务不存在".to_string())?;
        set_todo_completed_in_connection(&mut connection, id, completed)
    };
    refresh_badge_after_success(&app, &result);
    app.emit_to("main", "todos-changed", ())
        .map_err(|error| error.to_string())?;
    result
}

#[tauri::command]
pub fn update_todo_title(
    id: i64,
    title: String,
    repeat_scope: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoEdit, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        update_todo_title_in_connection(&mut connection, id, &title, repeat_scope.as_deref())
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn update_todo_note(
    id: i64,
    note: Option<String>,
    repeat_scope: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoEdit, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        update_todo_note_in_connection(&mut connection, id, note, repeat_scope.as_deref())
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
pub fn set_todo_priority(
    id: i64,
    priority: i64,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Todo, String> {
    let result = {
        let connection = lock_database(&database)?;
        set_todo_priority_in_connection(&connection, id, priority)
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
    repeat_scope: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoEdit, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        set_todo_schedule_in_connection(
            &mut connection,
            id,
            due_date,
            due_at,
            reminder_at,
            repeat_rule,
            repeat_scope.as_deref(),
        )
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn set_todo_group(
    id: i64,
    group_uuid: Option<String>,
    repeat_scope: Option<String>,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<TodoEdit, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        set_todo_group_in_connection(&mut connection, id, group_uuid, repeat_scope.as_deref())
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
pub fn archive_completed_todos(
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<usize, String> {
    let result = {
        let connection = lock_database(&database)?;
        archive_completed_todos_in_connection(&connection)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub fn hide_panel(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_focus_window(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("focus") else {
        return Err("专注窗口未初始化".to_string());
    };
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn hide_focus_window(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("focus") else {
        return Ok(());
    };
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_focus_window_compact(app: AppHandle, compact: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("focus") else {
        return Ok(());
    };

    let min_size = if compact {
        LogicalSize::new(FOCUS_COMPACT_MIN_WIDTH, FOCUS_COMPACT_MIN_HEIGHT)
    } else {
        LogicalSize::new(FOCUS_WINDOW_MIN_WIDTH, FOCUS_WINDOW_MIN_HEIGHT)
    };
    let size = if compact {
        LogicalSize::new(FOCUS_COMPACT_WIDTH, FOCUS_COMPACT_HEIGHT)
    } else {
        LogicalSize::new(FOCUS_WINDOW_WIDTH, FOCUS_WINDOW_HEIGHT)
    };

    if compact {
        window
            .set_min_size(Some(Size::Logical(min_size)))
            .map_err(|error| error.to_string())?;
        window
            .set_size(Size::Logical(size))
            .map_err(|error| error.to_string())?;
    } else {
        window
            .set_size(Size::Logical(size))
            .map_err(|error| error.to_string())?;
        window
            .set_min_size(Some(Size::Logical(min_size)))
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn publish_focus_notification(app: AppHandle, completed_phase: String) -> Result<(), String> {
    reminders::deliver_focus_notification(&app, &completed_phase)
}

#[tauri::command]
pub fn update_focus_tray_tooltip(
    app: AppHandle,
    phase: String,
    remaining_ms: u64,
    title: Option<String>,
) -> Result<(), String> {
    tray::update_focus_tooltip(&app, &phase, remaining_ms, title.as_deref());
    Ok(())
}

#[tauri::command]
pub fn restore_tray_task_tooltip(app: AppHandle) -> Result<(), String> {
    tray::update_task_badge(&app);
    Ok(())
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
pub async fn get_remote_sync_state(
    database: State<'_, Database>,
) -> Result<s3_sync::RemoteSyncState, String> {
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_manual_sync(&connection)?
    };
    s3_sync::get_remote_state(&prepared).await
}

#[tauri::command]
pub async fn upload_note_asset(
    attachment_uuid: String,
    file_name: String,
    content_type: String,
    expected_size: i64,
    expected_sha256: String,
    database: State<'_, Database>,
    runtime: State<'_, SyncRuntime>,
    asset_store: State<'_, NoteAssetStore>,
) -> Result<AssetUploadOutcome, String> {
    let bytes = asset_store.read_asset_file(
        &attachment_uuid,
        &file_name,
        expected_size,
        &expected_sha256,
    )?;
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_manual_sync(&connection)?
    };
    s3_sync::upload_immutable_asset(
        &runtime,
        &prepared,
        &attachment_uuid,
        &file_name,
        &bytes,
        &content_type,
        &expected_sha256,
    )
    .await
}

#[tauri::command]
pub async fn download_note_asset(
    attachment_uuid: String,
    file_name: String,
    expected_size: i64,
    expected_sha256: String,
    database: State<'_, Database>,
    runtime: State<'_, SyncRuntime>,
    asset_store: State<'_, NoteAssetStore>,
) -> Result<String, String> {
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_manual_sync(&connection)?
    };
    let bytes = s3_sync::download_asset_bytes(
        &runtime,
        &prepared,
        &attachment_uuid,
        &file_name,
        expected_size,
        &expected_sha256,
    )
    .await?;
    asset_store.write_downloaded_asset(
        &attachment_uuid,
        &file_name,
        &bytes,
        expected_size,
        &expected_sha256,
    )
}

#[tauri::command]
pub async fn delete_remote_note_asset(
    attachment_uuid: String,
    file_name: String,
    expected_size: i64,
    expected_sha256: String,
    database: State<'_, Database>,
    runtime: State<'_, SyncRuntime>,
) -> Result<bool, String> {
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_manual_sync(&connection)?
    };
    s3_sync::delete_asset_if_matches(
        &runtime,
        &prepared,
        &attachment_uuid,
        &file_name,
        expected_size,
        &expected_sha256,
    )
    .await
}

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    database: State<'_, Database>,
    runtime: State<'_, SyncRuntime>,
    asset_store: State<'_, NoteAssetStore>,
) -> Result<ManualSyncResult, String> {
    let _guard = runtime.acquire()?;
    let prepared = {
        let connection = lock_database(&database)?;
        s3_sync::prepare_manual_sync(&connection)?
    };
    let mut todo_remote = s3_sync::download_remote(&prepared).await?;
    let mut todo_conflict_retried = false;
    let todo_count = loop {
        let merged = {
            let mut connection = lock_database(&database)?;
            match &todo_remote.document {
                Some(document) => {
                    sync::merge_remote_document(&mut connection, document, now_millis())?
                }
                None => sync::build_document(&connection, now_millis())?,
            }
        };
        tray::update_task_badge(&app);
        let _ = app.emit_to("main", "todos-changed", ());

        match s3_sync::upload_document(&prepared, &merged, &todo_remote).await? {
            UploadOutcome::Success => break merged.todos.len(),
            UploadOutcome::Conflict if !todo_conflict_retried => {
                todo_conflict_retried = true;
                todo_remote = s3_sync::download_remote(&prepared).await?;
            }
            UploadOutcome::Conflict => {
                return Err("远端文件持续发生变化，已停止上传并保留本地数据".to_string());
            }
        }
    };

    let mut note_remote = s3_sync::download_note_remote(&prepared)
        .await
        .map_err(|error| format!("便签同步失败：{error}"))?;
    let mut note_conflict_retried = false;
    let note_count = loop {
        let merged = {
            let mut connection = lock_database(&database)?;
            match &note_remote.document {
                Some(document) => {
                    note_sync::merge_remote_document(&mut connection, document, now_millis())?
                }
                None => note_sync::build_document(&connection, now_millis())?,
            }
        };
        match s3_sync::upload_note_document(&prepared, &merged, &note_remote)
            .await
            .map_err(|error| format!("便签同步失败：{error}"))?
        {
            UploadOutcome::Success => {
                let _ = app.emit_to("main", "notes-changed", ());
                break merged.notes.len();
            }
            UploadOutcome::Conflict if !note_conflict_retried => {
                note_conflict_retried = true;
                note_remote = s3_sync::download_note_remote(&prepared)
                    .await
                    .map_err(|error| format!("便签同步失败：{error}"))?;
            }
            UploadOutcome::Conflict => {
                return Err("便签远端文件持续发生变化，已停止上传并保留本地数据".to_string());
            }
        }
    };

    let pending_attachments = {
        let connection = lock_database(&database)?;
        note_attachments::list_pending_transfers(&connection)?
    };
    let pending_attachment_count_before = pending_attachments.len();
    for attachment in pending_attachments {
        {
            let connection = lock_database(&database)?;
            note_attachments::set_transfer_state(
                &connection,
                &attachment.uuid,
                "uploading",
                None,
                false,
            )?;
        }
        let upload_result: Result<(), String> = async {
            let original = asset_store.read_asset_file(
                &attachment.uuid,
                "original",
                attachment.byte_size,
                &attachment.sha256,
            )?;
            s3_sync::upload_immutable_asset(
                &runtime,
                &prepared,
                &attachment.uuid,
                "original",
                &original,
                &attachment.mime_type,
                &attachment.sha256,
            )
            .await?;
            if attachment.kind == "image" {
                let preview_size = attachment
                    .preview_byte_size
                    .ok_or_else(|| "图片附件缺少预览大小".to_string())?;
                let preview_sha256 = attachment
                    .preview_sha256
                    .as_deref()
                    .ok_or_else(|| "图片附件缺少预览摘要".to_string())?;
                let preview = asset_store.read_asset_file(
                    &attachment.uuid,
                    "preview.jpg",
                    preview_size,
                    preview_sha256,
                )?;
                s3_sync::upload_immutable_asset(
                    &runtime,
                    &prepared,
                    &attachment.uuid,
                    "preview.jpg",
                    &preview,
                    "image/jpeg",
                    preview_sha256,
                )
                .await?;
            }
            Ok(())
        }
        .await;
        let connection = lock_database(&database)?;
        match upload_result {
            Ok(()) => {
                note_attachments::set_transfer_state(
                    &connection,
                    &attachment.uuid,
                    "uploaded",
                    None,
                    true,
                )?;
            }
            Err(error) => {
                note_attachments::set_transfer_state(
                    &connection,
                    &attachment.uuid,
                    "failed",
                    Some(&error),
                    false,
                )?;
                return Err(format!(
                    "附件二进制同步失败（待同步 {pending_attachment_count_before} 个）：{error}"
                ));
            }
        }
    }

    let mut attachment_remote = s3_sync::download_note_attachment_remote(&prepared)
        .await
        .map_err(|error| format!("附件元数据同步失败：{error}"))?;
    let mut attachment_conflict_retried = false;
    let (note_attachment_count, synced_attachment_document) = loop {
        let merged = {
            let mut connection = lock_database(&database)?;
            match &attachment_remote.document {
                Some(document) => note_attachment_sync::merge_remote_document(
                    &mut connection,
                    document,
                    now_millis(),
                )?,
                None => note_attachment_sync::build_document(&connection, now_millis())?,
            }
        };
        match s3_sync::upload_note_attachment_document(&prepared, &merged, &attachment_remote)
            .await
            .map_err(|error| format!("附件元数据同步失败：{error}"))?
        {
            UploadOutcome::Success => break (merged.attachments.len(), merged),
            UploadOutcome::Conflict if !attachment_conflict_retried => {
                attachment_conflict_retried = true;
                attachment_remote = s3_sync::download_note_attachment_remote(&prepared)
                    .await
                    .map_err(|error| format!("附件元数据同步失败：{error}"))?;
            }
            UploadOutcome::Conflict => {
                return Err("附件元数据持续发生变化，已停止上传并保留本地数据".to_string());
            }
        }
    };
    let pending_attachment_count = {
        let connection = lock_database(&database)?;
        note_attachment_sync::mark_document_synced(&connection, &synced_attachment_document)?;
        note_attachments::list_pending_transfers(&connection)?.len()
    };
    let _ = app.emit_to("main", "notes-changed", ());

    let state = s3_sync::get_remote_state(&prepared).await.ok();
    let conflict_retried =
        todo_conflict_retried || note_conflict_retried || attachment_conflict_retried;
    Ok(ManualSyncResult {
        message: if conflict_retried {
            "检测到远端更新，重新合并后同步完成".to_string()
        } else {
            "任务、便签和附件同步完成".to_string()
        },
        todo_count,
        note_count,
        note_attachment_count,
        pending_attachment_count,
        conflict_retried,
        todo_remote_etag: state.as_ref().and_then(|value| value.todo_etag.clone()),
        note_remote_etag: state.as_ref().and_then(|value| value.note_etag.clone()),
        note_attachment_remote_etag: state.and_then(|value| value.note_attachment_etag),
    })
}

fn refresh_badge_after_success<T>(app: &AppHandle, result: &Result<T, String>) {
    if result.is_ok() {
        tray::update_task_badge(app);
    }
}

fn emit_notes_changed_after_success<T>(app: &AppHandle, result: &Result<T, String>) {
    if result.is_ok() {
        let _ = app.emit_to("main", "notes-changed", ());
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
                id, uuid, title, note, group_uuid, completed, pinned, priority, sort_order,
                created_at, updated_at, completed_at, deleted_at, archived_at,
                due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid
            FROM todos
            WHERE deleted_at IS NULL AND archived_at IS NULL
            ORDER BY completed ASC, pinned DESC, sort_order ASC, created_at DESC, uuid DESC
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
            WHERE deleted_at IS NULL AND archived_at IS NULL
            ",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;

    connection
        .execute(
            "
            INSERT INTO todos (
                uuid, title, note, group_uuid, completed, pinned, priority, sort_order,
                created_at, updated_at, completed_at, deleted_at, archived_at,
                due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid, updated_by
            )
            VALUES (?1, ?2, NULL, ?3, 0, 0, 0, ?4, ?5, ?5, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?6)
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
            WHERE group_uuid = ?3 AND deleted_at IS NULL AND archived_at IS NULL
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
              AND archived_at IS NULL
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
    connection: &mut Connection,
    id: i64,
    title: &str,
    repeat_scope: Option<&str>,
) -> Result<TodoEdit, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务内容不能为空".to_string());
    }
    let target = find_todo(connection, id)?
        .filter(|todo| todo.deleted_at.is_none() && todo.archived_at.is_none())
        .ok_or_else(|| "任务不存在".to_string())?;
    let scope = normalize_repeat_edit_scope(repeat_scope)?;
    let ids = repeat_edit_ids(connection, &target, scope)?;
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;

    let transaction = connection.transaction().map_err(database_error)?;
    let mut changed = 0;
    for todo_id in &ids {
        changed += transaction
            .execute(
                "
            UPDATE todos
            SET title = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
                params![title, now, updated_by, todo_id],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    Ok(TodoEdit {
        updated_todos: find_todos_by_ids(connection, &ids)?,
    })
}

fn update_todo_note_in_connection(
    connection: &mut Connection,
    id: i64,
    note: Option<String>,
    repeat_scope: Option<&str>,
) -> Result<TodoEdit, String> {
    let note = normalize_todo_note(note)?;
    let target = find_todo(connection, id)?
        .filter(|todo| todo.deleted_at.is_none() && todo.archived_at.is_none())
        .ok_or_else(|| "任务不存在".to_string())?;
    let scope = normalize_repeat_edit_scope(repeat_scope)?;
    let ids = repeat_edit_ids(connection, &target, scope)?;
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;

    let transaction = connection.transaction().map_err(database_error)?;
    let mut changed = 0;
    for todo_id in &ids {
        changed += transaction
            .execute(
                "
            UPDATE todos
            SET note = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
                params![note, now, updated_by, todo_id],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    Ok(TodoEdit {
        updated_todos: find_todos_by_ids(connection, &ids)?,
    })
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
              AND archived_at IS NULL
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

fn set_todo_priority_in_connection(
    connection: &Connection,
    id: i64,
    priority: i64,
) -> Result<Todo, String> {
    let priority = normalize_todo_priority(priority)?;
    let changed = connection
        .execute(
            "
            UPDATE todos
            SET priority = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
            params![
                priority,
                now_millis(),
                device_id(connection).map_err(database_error)?,
                id
            ],
        )
        .map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    find_todo(connection, id)?.ok_or_else(|| "更新重要级别后未能读取任务".to_string())
}

fn set_todo_schedule_in_connection(
    connection: &mut Connection,
    id: i64,
    due_date: Option<String>,
    due_at: Option<i64>,
    reminder_at: Option<i64>,
    repeat_rule: Option<String>,
    repeat_scope: Option<&str>,
) -> Result<TodoEdit, String> {
    let due_date = normalize_due_date(due_date)?;
    let repeat_rule = normalize_repeat_rule(repeat_rule)?;
    if due_at.is_some_and(|value| value < 0) || reminder_at.is_some_and(|value| value < 0) {
        return Err("到期或提醒时间无效".to_string());
    }
    if due_date.is_some() && due_at.is_some() {
        return Err("纯日期任务不能同时设置具体到期时间".to_string());
    }
    if repeat_rule.is_some() && due_date.is_none() && due_at.is_none() {
        return Err("重复任务需要先设置到期时间".to_string());
    }
    let current = find_todo(connection, id)?
        .filter(|todo| todo.deleted_at.is_none() && todo.archived_at.is_none())
        .ok_or_else(|| "任务不存在".to_string())?;
    let scope = normalize_repeat_edit_scope(repeat_scope)?;
    let ids = repeat_edit_ids(connection, &current, scope)?;
    let repeat_due_date = match (&repeat_rule, &due_date, due_at) {
        (Some(_), Some(date), _) => Some(date.clone()),
        (Some(_), None, Some(timestamp)) => Some(local_date_from_timestamp(timestamp)?),
        _ => None,
    };
    let repeat_next_due_date = match (&repeat_due_date, &repeat_rule) {
        (Some(date), Some(rule)) => Some(next_repeat_due_date(date, rule)?),
        _ => None,
    };
    let repeat_series_uuid = repeat_rule.as_ref().map(|_| {
        current
            .repeat_series_uuid
            .clone()
            .unwrap_or(current.uuid.clone())
    });
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;

    let transaction = connection.transaction().map_err(database_error)?;
    let mut changed = 0;
    for todo_id in &ids {
        changed += transaction
            .execute(
                "
            UPDATE todos
            SET due_date = ?1, due_at = ?2, reminder_at = ?3,
                repeat_rule = ?4, repeat_next_due_date = ?5,
                repeat_series_uuid = ?6,
                updated_at = ?7, updated_by = ?8
            WHERE id = ?9 AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
                params![
                    due_date,
                    due_at,
                    reminder_at,
                    repeat_rule,
                    repeat_next_due_date,
                    repeat_series_uuid,
                    now,
                    updated_by,
                    todo_id
                ],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    Ok(TodoEdit {
        updated_todos: find_todos_by_ids(connection, &ids)?,
    })
}

fn set_todo_group_in_connection(
    connection: &mut Connection,
    id: i64,
    group_uuid: Option<String>,
    repeat_scope: Option<&str>,
) -> Result<TodoEdit, String> {
    let group_uuid = normalize_group_uuid(connection, group_uuid)?;
    let target = find_todo(connection, id)?
        .filter(|todo| todo.deleted_at.is_none() && todo.archived_at.is_none())
        .ok_or_else(|| "任务不存在".to_string())?;
    let scope = normalize_repeat_edit_scope(repeat_scope)?;
    let ids = repeat_edit_ids(connection, &target, scope)?;
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;

    let transaction = connection.transaction().map_err(database_error)?;
    let mut changed = 0;
    for todo_id in &ids {
        changed += transaction
            .execute(
                "
            UPDATE todos
            SET group_uuid = ?1, updated_at = ?2, updated_by = ?3
            WHERE id = ?4 AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
                params![group_uuid, now, updated_by, todo_id],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;

    if changed == 0 {
        return Err("任务不存在".to_string());
    }

    Ok(TodoEdit {
        updated_todos: find_todos_by_ids(connection, &ids)?,
    })
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
                  AND archived_at IS NULL
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
        .filter(|todo| todo.deleted_at.is_none() && todo.archived_at.is_none())
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
                    AND archived_at IS NULL
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
                  AND archived_at IS NULL
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
            WHERE completed = 1 AND deleted_at IS NULL AND archived_at IS NULL
            ",
            params![now, updated_by],
        )
        .map_err(database_error)
}

fn archive_completed_todos_in_connection(connection: &Connection) -> Result<usize, String> {
    let now = now_millis();
    let updated_by = device_id(connection).map_err(database_error)?;
    connection
        .execute(
            "
            UPDATE todos
            SET archived_at = ?1, updated_at = ?1, updated_by = ?2
            WHERE completed = 1 AND deleted_at IS NULL AND archived_at IS NULL
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
                id, uuid, title, note, group_uuid, completed, pinned, priority, sort_order,
                created_at, updated_at, completed_at, deleted_at, archived_at,
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

fn find_todo_id_by_uuid(connection: &Connection, uuid: &str) -> Result<Option<i64>, String> {
    connection
        .query_row(
            "
            SELECT id
            FROM todos
            WHERE uuid = ?1
              AND deleted_at IS NULL
              AND archived_at IS NULL
            ",
            params![uuid],
            |row| row.get(0),
        )
        .optional()
        .map_err(database_error)
}

fn find_todos_by_ids(connection: &Connection, ids: &[i64]) -> Result<Vec<Todo>, String> {
    ids.iter()
        .map(|id| find_todo(connection, *id)?.ok_or_else(|| "更新后未能读取任务".to_string()))
        .collect()
}

fn repeat_edit_ids(
    connection: &Connection,
    target: &Todo,
    scope: RepeatEditScope,
) -> Result<Vec<i64>, String> {
    if scope == RepeatEditScope::Single {
        return Ok(vec![target.id]);
    }

    let boundary_due_date = match (target.due_date.as_deref(), target.due_at) {
        (Some(date), _) => date.to_string(),
        (None, Some(timestamp)) => local_date_from_timestamp(timestamp)?,
        (None, None) => return Ok(vec![target.id]),
    };
    let Some(series_uuid) = target.repeat_series_uuid.as_deref().or_else(|| {
        if target.repeat_rule.is_some() {
            Some(target.uuid.as_str())
        } else {
            None
        }
    }) else {
        return Ok(vec![target.id]);
    };

    let mut statement = connection
        .prepare(
            "
            SELECT DISTINCT id
            FROM todos
            WHERE deleted_at IS NULL
              AND archived_at IS NULL
              AND (
                id = ?1
                OR (
                  completed = 0
                  AND COALESCE(
                    due_date,
                    strftime('%Y-%m-%d', due_at / 1000, 'unixepoch', 'localtime')
                  ) >= ?2
                  AND (repeat_series_uuid = ?3 OR uuid = ?3)
                )
              )
            ORDER BY COALESCE(
              due_date,
              strftime('%Y-%m-%d', due_at / 1000, 'unixepoch', 'localtime')
            ) ASC, created_at ASC, id ASC
            ",
        )
        .map_err(database_error)?;
    let ids = statement
        .query_map(params![target.id, boundary_due_date, series_uuid], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    if ids.is_empty() {
        Ok(vec![target.id])
    } else {
        Ok(ids)
    }
}

fn map_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: row.get(0)?,
        uuid: row.get(1)?,
        title: row.get(2)?,
        note: row.get(3)?,
        group_uuid: row.get(4)?,
        completed: row.get::<_, i64>(5)? != 0,
        pinned: row.get::<_, i64>(6)? != 0,
        priority: row.get(7)?,
        sort_order: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        completed_at: row.get(11)?,
        deleted_at: row.get(12)?,
        archived_at: row.get(13)?,
        due_date: row.get(14)?,
        due_at: row.get(15)?,
        reminder_at: row.get(16)?,
        repeat_rule: row.get(17)?,
        repeat_next_due_date: row.get(18)?,
        repeat_series_uuid: row.get(19)?,
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

fn normalize_todo_note(note: Option<String>) -> Result<Option<String>, String> {
    let Some(note) = note else {
        return Ok(None);
    };
    let note = note.trim();
    if note.is_empty() {
        return Ok(None);
    }
    if note.chars().count() > TODO_NOTE_MAX_CHARS {
        return Err(format!("备注不能超过 {TODO_NOTE_MAX_CHARS} 个字符"));
    }
    Ok(Some(note.to_string()))
}

fn normalize_group_color(color: &str) -> Result<&str, String> {
    let color = color.trim();
    match color {
        "yellow" | "green" | "blue" | "peach" | "lavender" | "gray" => Ok(color),
        _ => Err("分组颜色不在预设范围内".to_string()),
    }
}

fn normalize_todo_priority(priority: i64) -> Result<i64, String> {
    match priority {
        0 | 1 => Ok(priority),
        _ => Err("任务重要级别无效".to_string()),
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

fn normalize_repeat_edit_scope(repeat_scope: Option<&str>) -> Result<RepeatEditScope, String> {
    match repeat_scope.unwrap_or("single") {
        "single" => Ok(RepeatEditScope::Single),
        "future" => Ok(RepeatEditScope::Future),
        _ => Err("编辑范围无效".to_string()),
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
    let (due_date, due_at) = if source.due_at.is_some() {
        (
            None,
            Some(timestamp_for_local_date(next_due_date, source.due_at)?),
        )
    } else {
        (Some(next_due_date), None)
    };
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
            WHERE deleted_at IS NULL AND archived_at IS NULL
            ",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)?;

    connection
        .execute(
            "
            INSERT INTO todos (
                uuid, title, note, group_uuid, completed, pinned, priority, sort_order,
                created_at, updated_at, completed_at, deleted_at,
                archived_at, due_date, due_at, reminder_at,
                repeat_rule, repeat_next_due_date, repeat_series_uuid, updated_by
            )
            VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?8, NULL, NULL, NULL, ?9, ?10, NULL, ?11, ?12, ?13, ?14)
            ",
            params![
                uuid,
                source.title,
                source.note,
                source.group_uuid,
                source.pinned,
                source.priority,
                sort_order,
                now,
                due_date,
                due_at,
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

    fn only_updated(mut edit: TodoEdit) -> Todo {
        assert_eq!(edit.updated_todos.len(), 1);
        edit.updated_todos.remove(0)
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
    fn updates_todo_note_with_normalization() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "task", None).unwrap();

        let with_note = only_updated(
            update_todo_note_in_connection(
                &mut connection,
                created.id,
                Some("  detail  ".to_string()),
                None,
            )
            .unwrap(),
        );
        assert_eq!(with_note.note.as_deref(), Some("detail"));

        let cleared = only_updated(
            update_todo_note_in_connection(
                &mut connection,
                created.id,
                Some("   ".to_string()),
                None,
            )
            .unwrap(),
        );
        assert_eq!(cleared.note, None);

        let long_note = "x".repeat(TODO_NOTE_MAX_CHARS + 1);
        let error =
            update_todo_note_in_connection(&mut connection, created.id, Some(long_note), None)
                .unwrap_err();
        assert!(error.contains("备注不能超过"));
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

        let edited = only_updated(
            update_todo_title_in_connection(&mut connection, first.id, "  edited  ", None).unwrap(),
        );
        assert_eq!(edited.title, "edited");
        assert!(update_todo_title_in_connection(&mut connection, first.id, " ", None).is_err());

        let pinned = set_todo_pinned_in_connection(&connection, first.id, true).unwrap();
        assert!(pinned.pinned);
        assert_eq!(
            list_todos_from_connection(&connection).unwrap()[0].id,
            first.id
        );

        let scheduled = only_updated(
            set_todo_schedule_in_connection(
                &mut connection,
                first.id,
                Some("2026-06-10".to_string()),
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        );
        assert_eq!(scheduled.due_date.as_deref(), Some("2026-06-10"));
        assert!(set_todo_schedule_in_connection(
            &mut connection,
            first.id,
            Some("2026/06/10".to_string()),
            None,
            None,
            None,
            None
        )
        .is_err());
        assert!(set_todo_schedule_in_connection(
            &mut connection,
            first.id,
            Some("2026-02-31".to_string()),
            None,
            None,
            None,
            None
        )
        .is_err());
        let repeating = only_updated(
            set_todo_schedule_in_connection(
                &mut connection,
                first.id,
                Some("2026-06-10".to_string()),
                None,
                None,
                Some("daily".to_string()),
                None,
            )
            .unwrap(),
        );
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
    fn archives_completed_todos_without_deleting_them() {
        let mut connection = connection();
        let active = create_todo_in_connection(&connection, "active", None).unwrap();
        let completed = create_todo_in_connection(&connection, "done", None).unwrap();
        set_todo_completed_in_connection(&mut connection, completed.id, true).unwrap();

        assert_eq!(
            archive_completed_todos_in_connection(&connection).unwrap(),
            1
        );

        let visible = list_todos_from_connection(&connection).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, active.id);

        let archived = find_todo(&connection, completed.id)
            .unwrap()
            .expect("archived todo");
        assert!(archived.completed);
        assert_eq!(archived.deleted_at, None);
        assert!(archived.archived_at.is_some());
    }

    #[test]
    fn completing_repeating_todo_creates_only_the_next_instance() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "repeat", None).unwrap();
        let repeating = only_updated(
            set_todo_schedule_in_connection(
                &mut connection,
                created.id,
                Some("2026-06-12".to_string()),
                None,
                None,
                Some("weekdays".to_string()),
                None,
            )
            .unwrap(),
        );
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
    fn completing_timed_repeating_todo_preserves_due_time() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "timed repeat", None).unwrap();
        let due_at = timestamp_for_local_date("2026-06-12", None).unwrap();
        let repeating = only_updated(
            set_todo_schedule_in_connection(
                &mut connection,
                created.id,
                None,
                Some(due_at),
                None,
                Some("weekdays".to_string()),
                None,
            )
            .unwrap(),
        );

        assert_eq!(repeating.due_date, None);
        assert_eq!(repeating.due_at, Some(due_at));
        assert_eq!(
            repeating.repeat_next_due_date.as_deref(),
            Some("2026-06-15")
        );

        let next = set_todo_completed_in_connection(&mut connection, created.id, true)
            .unwrap()
            .created_todo
            .expect("next timed repeat instance");
        assert_eq!(next.due_date, None);
        assert_eq!(
            next.due_at,
            Some(timestamp_for_local_date("2026-06-15", Some(due_at)).unwrap())
        );
        assert_eq!(next.repeat_next_due_date.as_deref(), Some("2026-06-16"));
    }

    #[test]
    fn deleting_repeating_series_soft_deletes_all_instances() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "repeat", None).unwrap();
        let repeating = only_updated(
            set_todo_schedule_in_connection(
                &mut connection,
                created.id,
                Some("2026-06-10".to_string()),
                None,
                None,
                Some("daily".to_string()),
                None,
            )
            .unwrap(),
        );
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
    fn editing_future_repeating_todos_updates_active_future_instances() {
        let mut connection = connection();
        let created = create_todo_in_connection(&connection, "repeat", None).unwrap();
        let repeating = only_updated(
            set_todo_schedule_in_connection(
                &mut connection,
                created.id,
                Some("2026-06-10".to_string()),
                None,
                None,
                Some("daily".to_string()),
                None,
            )
            .unwrap(),
        );
        let next = set_todo_completed_in_connection(&mut connection, repeating.id, true)
            .unwrap()
            .created_todo
            .expect("next repeat instance");

        let edited = update_todo_title_in_connection(
            &mut connection,
            repeating.id,
            "future title",
            Some("future"),
        )
        .unwrap();

        assert_eq!(edited.updated_todos.len(), 2);
        let titles = list_todos_from_connection(&connection)
            .unwrap()
            .into_iter()
            .filter(|todo| todo.id == repeating.id || todo.id == next.id)
            .map(|todo| todo.title)
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["future title", "future title"]);
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
        assert_eq!(
            next_repeat_due_date("2026-12-31", "daily").unwrap(),
            "2027-01-01"
        );
        assert_eq!(
            next_repeat_due_date("2026-12-25", "weekly").unwrap(),
            "2027-01-01"
        );
        assert_eq!(
            next_repeat_due_date("2026-10-30", "weekdays").unwrap(),
            "2026-11-02"
        );
        assert_eq!(
            next_repeat_due_date("2028-02-29", "monthly").unwrap(),
            "2028-03-29"
        );
    }

    #[test]
    fn creates_groups_and_moves_todos_between_them() {
        let mut connection = connection();
        let group = create_group_in_connection(&connection, "  工作  ").unwrap();
        assert_eq!(group.name, "工作");
        assert!(Uuid::parse_str(&group.uuid).is_ok());

        let created =
            create_todo_in_connection(&connection, "grouped", Some(group.uuid.clone())).unwrap();
        assert_eq!(created.group_uuid.as_deref(), Some(group.uuid.as_str()));

        let ungrouped = only_updated(
            set_todo_group_in_connection(&mut connection, created.id, None, None).unwrap(),
        );
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
