use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    task_progress_store::{
        self as store, ProgressCount, ProgressCursor, ProgressPage, ProgressWrite,
    },
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_task_progress(
    database: State<'_, Database>,
    task_uuid: String,
    cursor: Option<ProgressCursor>,
) -> Result<ProgressPage, String> {
    let mut db = lock_database(&database).map_err(|_| "PROGRESS_DATABASE")?;
    store::list(&mut db, &task_uuid, cursor.as_ref())
}
#[tauri::command]
pub fn write_task_progress(
    database: State<'_, Database>,
    app: AppHandle,
    request: ProgressWrite,
) -> Result<ProgressPage, String> {
    let result = {
        let mut db = lock_database(&database).map_err(|_| "PROGRESS_DATABASE")?;
        let by = device_id(&db).map_err(|_| "PROGRESS_DATABASE")?;
        store::write(&mut db, &request, now_millis(), &by)?
    };
    let _ = app.emit_to("main", "todos-changed", ());
    let _ = app.emit_to("main", "task-progress-changed", ());
    Ok(result)
}
#[tauri::command]
pub fn count_task_progress(
    database: State<'_, Database>,
    task_uuids: Vec<String>,
) -> Result<Vec<ProgressCount>, String> {
    let db = lock_database(&database).map_err(|_| "PROGRESS_DATABASE")?;
    store::counts(&db, &task_uuids)
}
#[tauri::command]
pub fn dismiss_task_progress_notice(
    database: State<'_, Database>,
    app: AppHandle,
    task_uuid: String,
) -> Result<(), String> {
    {
        let db = lock_database(&database).map_err(|_| "PROGRESS_DATABASE")?;
        store::dismiss_notice(&db, &task_uuid)?;
    }
    let _ = app.emit_to("main", "task-progress-changed", ());
    Ok(())
}
