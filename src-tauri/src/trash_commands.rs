use crate::{
    commands::lock_database,
    db::{device_id, now_millis, Database},
    trash::{self, TrashItem, TrashKind},
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_trash(
    database: State<'_, Database>,
    offset: u32,
    limit: u32,
) -> Result<Vec<TrashItem>, String> {
    let connection = lock_database(&database)?;
    trash::list(&connection, offset, limit)
}

#[tauri::command]
pub fn preview_trash(
    database: State<'_, Database>,
    kind: TrashKind,
    uuid: String,
) -> Result<TrashItem, String> {
    let connection = lock_database(&database)?;
    trash::preview(&connection, kind, &uuid)
}

#[tauri::command]
pub fn restore_trash(
    database: State<'_, Database>,
    app: AppHandle,
    expected: TrashItem,
) -> Result<(), String> {
    {
        let mut connection = lock_database(&database)?;
        let by = device_id(&connection).map_err(|_| "TRASH_DATABASE_FAILED".to_string())?;
        trash::restore(&mut connection, &expected, now_millis(), &by)?;
    }
    crate::tray::update_task_badge(&app);
    let _ = app.emit_to("main", "todos-changed", ());
    let _ = app.emit_to("main", "notes-changed", ());
    Ok(())
}
